//! STAB Flash Loan Blueprint
//!
//! This blueprint allows users to borrow STAB tokens from the Stabilis component. The user must pay back the borrowed amount plus interest in the same transaction.
//! This works by the user receiving a transient token loan receipt, that only the FlashLoan component can burn. They have to pay this back in the same transaction to make the transaction succeed.

use crate::stabilis_component::stabilis_component::*;
use scrypto::prelude::*;

/// A receipt recording the flash loan
#[derive(ScryptoSbor, NonFungibleData)]
pub struct LoanReceipt {
    #[mutable]
    pub borrowed_amount: Decimal,
    pub interest: Decimal,
}

#[blueprint]
#[types(Decimal)]
mod flash_loans {
    enable_method_auth! {
        methods {
            borrow => restrict_to: [OWNER];
            settings => restrict_to: [OWNER];
            pay_back => restrict_to: [OWNER];
            retrieve_interest => restrict_to: [OWNER];
        }
    }

    struct FlashLoans {
        /// The vault for the controller badge, used to authorize minting STAB
        badge_vault: FungibleVault,
        /// The resource manager for the loan receipts
        loan_receipt_manager: ResourceManager,
        /// The vault for the interest
        interest_vault: Option<Vault>,
        /// The counter for the loan receipts
        loan_receipt_counter: u64,
        /// The interest rate for the flash loans (starts at 0.00, so example: 0.05 is 5% interest)
        interest: Decimal,
        /// The global instance of the Stabilis component
        stabilis: Global<Stabilis>,
        /// Whether flash loans are possible now
        enabled: bool,
        /// The amount of STAB tokens loaned
        amount_loaned: Decimal,
    }

    impl FlashLoans {
        /// Instantiates the FlashLoans component
        ///
        /// # Input
        /// - `controller_badge`: The controller badge of the Stabilis component
        /// - `stabilis`: The global instance of the Stabilis component
        ///
        /// # Output
        ///  - The global instance of the FlashLoans component
        ///
        /// # Logic
        /// - Creates a ResourceManager for the loan receipts
        ///     - with depositor said to `deny_all`, making it transient (i.e. it needs to be burned in the same transaction as the borrow method is called)
        ///         - with burner said to only be allowed by this component
        /// - Instantiates the FlashLoans component
        pub fn instantiate(
            controller_badge: Bucket,
            stabilis: Global<Stabilis>,
        ) -> Global<FlashLoans> {
            let (address_reservation, component_address) =
                Runtime::allocate_component_address(FlashLoans::blueprint_id());

            let loan_receipt_manager: ResourceManager =
                ResourceBuilder::new_integer_non_fungible::<LoanReceipt>(OwnerRole::Fixed(rule!(
                    require_amount(dec!("0.75"), controller_badge.resource_address())
                )))
                .metadata(metadata!(
                    init {
                        "name" => "STAB Flash Loan Receipt", locked;
                        "symbol" => "stabFLASH", locked;
                        "description" => "A receipt for your STAB flash loan", locked;
                        "info_url" => "https://stabilis.finance", updatable;
                    }
                ))
                .non_fungible_data_update_roles(non_fungible_data_update_roles!(
                    non_fungible_data_updater => rule!(require(global_caller(component_address)));
                    non_fungible_data_updater_updater => rule!(deny_all);
                ))
                .mint_roles(mint_roles!(
                    minter => rule!(require(global_caller(component_address)));
                    minter_updater => rule!(deny_all);
                ))
                .burn_roles(burn_roles!(
                    burner => rule!(require(global_caller(component_address)));
                    burner_updater => rule!(deny_all);
                ))
                .deposit_roles(deposit_roles!(
                    depositor => rule!(deny_all);
                    depositor_updater => rule!(deny_all);
                ))
                .create_with_no_initial_supply();

            let controller_address: ResourceAddress = controller_badge.resource_address();

            //create the flash loan component
            Self {
                badge_vault: FungibleVault::with_bucket(controller_badge.as_fungible()),
                loan_receipt_manager,
                interest: dec!(0),
                interest_vault: None,
                stabilis,
                loan_receipt_counter: 0,
                enabled: true,
                amount_loaned: dec!(0),
            }
            .instantiate()
            .prepare_to_globalize(OwnerRole::Fixed(rule!(require(controller_address))))
            .with_address(address_reservation)
            .globalize()
        }

        /// Alter the settings of the FlashLoans component, interest rate starts at 0.00, so example: 0.05 is 5% interest
        pub fn settings(&mut self, interest: Decimal, enabled: bool) {
            self.interest = interest;
            self.enabled = enabled;
        }

        /// Take out a flash loan of STAB tokens
        ///
        /// # Input
        /// - `amount`: The amount of STAB tokens to borrow
        ///
        /// # Output
        /// - The borrowed STAB in a `Bucket`
        /// - The loan receipt in a `Bucket`
        ///
        /// # Logic
        /// - Checks if flash loans are enabled
        /// - Increments the amount of STAB loaned
        /// - Creates a loan receipt
        /// - Mints the loan receipt
        /// - Mints the requested STAB tokens
        /// - Returns the STAB tokens and the loan receipt
        pub fn borrow(&mut self, amount: Decimal) -> (Bucket, Bucket) {
            assert!(self.enabled, "Flash loans are disabled.");
            self.amount_loaned += amount;
            let loan_receipt = LoanReceipt {
                borrowed_amount: amount,
                interest: self.interest,
            };

            let receipt: Bucket = self.loan_receipt_manager.mint_non_fungible(
                &NonFungibleLocalId::integer(self.loan_receipt_counter),
                loan_receipt,
            );
            self.loan_receipt_counter += 1;

            let loan_bucket: Bucket = self
                .badge_vault
                .authorize_with_amount(dec!("0.75"), || self.stabilis.free_stab(amount));

            (loan_bucket, receipt)
        }

        /// Pay back the STAB tokens borrowed in a flash loan
        /// (needs to be called in the same transaction as the borrow method because of the flash loan receipts transient nature)
        ///
        /// # Input
        /// - `receipt_bucket`: The loan receipt
        /// - `payment`: The STAB tokens to pay back (which includes the interest)
        ///
        /// # Output
        /// - The remaining STAB tokens after paying back the loan
        ///
        /// # Logic
        /// - Checks if the receipt is valid
        /// - Checks if the payment is enough to pay back the loan
        /// - Burns the receipt
        /// - Burns the STAB tokens borrowed
        /// - If there is interest, it is put into the interest vault
        /// - Returns the remaining STAB tokens
        pub fn pay_back(&mut self, receipt_bucket: Bucket, mut payment: Bucket) -> Bucket {
            assert!(
                receipt_bucket.resource_address() == self.loan_receipt_manager.address(),
                "Invalid receipt"
            );

            let receipt: LoanReceipt = self
                .loan_receipt_manager
                .get_non_fungible_data(&receipt_bucket.as_non_fungible().non_fungible_local_id());

            assert!(
                payment.amount() >= receipt.borrowed_amount * (dec!(1) + receipt.interest),
                "Not enough STAB paid back."
            );

            self.badge_vault.authorize_with_amount(dec!("0.75"), || {
                self.stabilis
                    .burn_stab(payment.take(receipt.borrowed_amount))
            });

            if receipt.interest > dec!(0) {
                if self.interest_vault.is_none() {
                    self.interest_vault = Some(Vault::with_bucket(
                        payment.take(receipt.interest * receipt.borrowed_amount),
                    ));
                } else {
                    self.interest_vault
                        .as_mut()
                        .unwrap()
                        .put(payment.take(receipt.interest * receipt.borrowed_amount));
                }
            }

            receipt_bucket.burn();

            payment
        }

        /// Method called to empty the interest vault
        pub fn retrieve_interest(&mut self) -> Bucket {
            self.interest_vault.as_mut().unwrap().take_all()
        }
    }
}
