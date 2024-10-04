//! # The STAB component Blueprint
//!
//! This module contains the Stabilis component, which is a smart contract that allows users to open, close, top up and liquidate loans of STAB tokens.
//! The STAB token is a stablecoin that is pegged to its own internal price, which is determined by an interest rate (meaning, this internal price is variable!).
//!
//! To open a loan, users must provide collateral in the form of accepted tokens, and receive STAB tokens in return.
//! The collateral must be worth more than the borrowed STAB tokens (at internal price) times a modifier (MCR), to ensure the loan is safe.
//! The value of STAB tokens is determined by the internal price, so a negative interest rate, for instance, means it is getting cheaper to borrow STAB tokens over time.
//! A loan can be paid off at any time by returning the STAB tokens.
//!
//! If collateral value < STAB tokens value * MCR, a loan can be marked for liquidation.
//! After a loan is marked, the borrower has a certain amount of time to save the loan by adding more collateral.
//! If not saved, the marker of the loan has the first opportunity to liquidate it.
//! If the marker does not liquidate the loan, anyone can liquidate it.
//! Liquidating a loan means taking part of the collateral and paying back the STAB tokens. The liquidator receives a fee for this.
//! If there's still collateral left after this fee, a fee is paid to the Stabilis component.
//! If there's still collateral left after this fee, the original borrower can retrieve the remaining collateral.
//!
//! To summarize, the typical life cycle of a loan, and the accompanying methods called on it:
//! - Open a loan: `open_cdp`
//! - Close a loan: `close_cdp`
//! - Add collateral to a loan: `top_up_cdp`
//! - Borrow more: `borrow_more`
//! - Partially close a loan: `partial_close_cdp`
//! - Force liquidate a loan (liquidate a loan immediately without it being undercollateralized): `force_liquidate`
//! - Force mint STAB tokens (force a borrower to mint more STAB tokens in return for collateral added to their CDP): `force_mint`
//! - Mark a loan to liquidate it: `mark_for_liquidation`
//! - Liquidate a loan: `liquidate_position_with_marker` or `liquidate_position_without_marker`
//! - Retrieve leftover collateral after being liquidated: `retrieve_leftover_collateral`

use crate::shared_structs::*;
use scrypto::prelude::*;
use scrypto_avltree::AvlTree;

#[blueprint]
#[types(
    ResourceAddress,
    bool,
    Decimal,
    CdpStatus,
    u64,
    CdpUpdate,
    Instant,
    NonFungibleLocalId,
    CollateralInfo,
    PoolUnitInfo,
    AvlTree<Decimal, Vec<NonFungibleLocalId>>
)]
mod stabilis_component {
    enable_method_auth! {
        methods {
            return_internal_price => PUBLIC;
            add_pool_collateral => restrict_to: [OWNER];
            open_cdp => restrict_to: [OWNER];
            top_up_cdp => restrict_to: [OWNER];
            close_cdp => restrict_to: [OWNER];
            borrow_more => restrict_to: [OWNER];
            partial_close_cdp => restrict_to: [OWNER];
            retrieve_leftover_collateral => restrict_to: [OWNER];
            mark_for_liquidation => restrict_to: [OWNER];
            liquidate_position_with_marker => restrict_to: [OWNER];
            liquidate_position_without_marker => restrict_to: [OWNER];
            change_collateral_price => restrict_to: [OWNER];
            empty_collateral_treasury => restrict_to: [OWNER];
            edit_collateral => restrict_to: [OWNER];
            edit_pool_collateral => restrict_to: [OWNER];
            mint_controller_badge => restrict_to: [OWNER];
            set_liquidation_delay => restrict_to: [OWNER];
            set_unmarked_delay => restrict_to: [OWNER];
            set_stops => restrict_to: [OWNER];
            set_max_vector_length => restrict_to: [OWNER];
            set_minimum_mint => restrict_to: [OWNER];
            set_fines => restrict_to: [OWNER];
            add_collateral => restrict_to: [OWNER];
            change_internal_price => restrict_to: [OWNER];
            remove_collateral => restrict_to: [OWNER];
            force_liquidate => restrict_to: [OWNER];
            force_mint => restrict_to: [OWNER];
            set_force_mint_multiplier => restrict_to: [OWNER];
            free_stab => restrict_to: [OWNER];
            burn_stab => restrict_to: [OWNER];
            burn_marker => restrict_to: [OWNER];
            burn_loan_receipt => restrict_to: [OWNER];
        }
    }
    struct Stabilis {
        /// KVS storing all accepted collaterals and their information
        collaterals: KeyValueStore<ResourceAddress, CollateralInfo>,
        /// KVS storing all accepted pool units and their information
        pool_units: KeyValueStore<ResourceAddress, PoolUnitInfo>,
        /// KVS storing all active collateral ratios for each collateral
        collateral_ratios:
            KeyValueStore<ResourceAddress, AvlTree<Decimal, Vec<NonFungibleLocalId>>>,
        /// Counter for the CDPs
        cdp_counter: u64,
        /// The resource manager for the CDPs
        cdp_manager: ResourceManager,
        /// The resource manager for the STAB token
        stab_manager: ResourceManager,
        /// The resource manager for the controller badge
        controller_badge_manager: ResourceManager,
        /// The internal price of STAB
        internal_stab_price: Decimal,
        /// The circulating supply of STAB
        circulating_stab: Decimal,
        /// The resource manager for the CDP markers
        cdp_marker_manager: ResourceManager,
        /// Counter for the CDP markers
        cdp_marker_counter: u64,
        /// AVL tree storing all marked CDPs (Basically used as a very big lazily-loadable Vec, as this data type doesn't exist yet)
        marked_cdps: AvlTree<Decimal, NonFungibleLocalId>,
        /// Counter for the active marked CDPs
        marked_cdps_active: u64,
        /// Counter for the marker placing in the AVL tree
        marker_placing_counter: Decimal,
        /// Resource manager for the liquidation receipts
        liquidation_receipt_manager: ResourceManager,
        /// Counter for the liquidation receipts
        liquidation_counter: u64,
        /// The protocol parameters
        parameters: ProtocolParameters,
    }

    impl Stabilis {
        /// Instantiates the Stabilis component
        ///
        /// # Output
        /// - The global instance of the Stabilis component
        /// - The controller badge for the Stabilis component
        ///
        /// # Logic
        /// - Sets the protocol parameters
        /// - Assigns a component address
        /// - Creates the controller badge
        /// - Creates the STAB token manager
        /// - Creates the CDP manager
        /// - Creates the CDP marker manager
        /// - Creates the liquidation receipt manager
        /// - Creates the Stabilis component
        pub fn instantiate() -> (Global<Stabilis>, Bucket) {
            let parameters = ProtocolParameters {
                minimum_mint: dec!(1),
                max_vector_length: 250,
                liquidation_delay: 5,
                unmarked_delay: 5,
                liquidation_liquidation_fine: dec!("0.10"),
                stabilis_liquidation_fine: dec!("0.05"),
                stop_liquidations: false,
                stop_openings: false,
                stop_closings: false,
                stop_force_mint: false,
                stop_force_liquidate: false,
                force_mint_cr_multiplier: dec!(3),
            };

            let (address_reservation, component_address) =
                Runtime::allocate_component_address(Stabilis::blueprint_id());

            let controller_role: Bucket = ResourceBuilder::new_fungible(OwnerRole::Fixed(rule!(
                require(global_caller(component_address))
            )))
            .divisibility(DIVISIBILITY_MAXIMUM)
            .metadata(metadata! (
                init {
                    "name" => "controller badge stabilis", locked;
                    "symbol" => "stabCTRL", locked;
                }
            ))
            .mint_roles(mint_roles!(
                minter => rule!(require(global_caller(component_address)));
                minter_updater => rule!(deny_all);
            ))
            .mint_initial_supply(10)
            .into();

            let controller_badge_manager: ResourceManager = controller_role.resource_manager();

            let stab_manager: ResourceManager = ResourceBuilder::new_fungible(OwnerRole::Fixed(
                rule!(require(controller_role.resource_address())),
            ))
            .divisibility(DIVISIBILITY_MAXIMUM)
            .metadata(metadata! (
                init {
                    "name" => "STAB token", updatable;
                    "symbol" => "STAB", updatable;
                    "info_url" => "https://ilikeitstable.com", updatable;
                    "icon_url" => Url::of("https://imgur.com/fEwyP5f.png"), updatable;
                }
            ))
            .mint_roles(mint_roles!(
                minter => rule!(require(global_caller(component_address))
                || require_amount(
                    dec!("0.75"),
                    controller_role.resource_address()
                ));
                minter_updater => rule!(require_amount(
                    dec!("0.75"),
                    controller_role.resource_address()
                ));
            ))
            .burn_roles(burn_roles!(
                burner => rule!(require(global_caller(component_address))
                || require_amount(
                    dec!("0.75"),
                    controller_role.resource_address()
                ));
                burner_updater => rule!(require_amount(
                    dec!("0.75"),
                    controller_role.resource_address()
                ));
            ))
            .create_with_no_initial_supply();

            let cdp_manager: ResourceManager =
                ResourceBuilder::new_integer_non_fungible::<Cdp>(OwnerRole::Fixed(rule!(
                    require_amount(dec!("0.75"), controller_role.resource_address())
                )))
                .metadata(metadata!(
                    init {
                        "name" => "Stabilis Loan Receipt", locked;
                        "symbol" => "stabLOAN", locked;
                        "description" => "A receipt for your Stabilis loan", locked;
                        "info_url" => "https://ilikeitstable.com", updatable;
                        "icon_url" => Url::of("https://i.imgur.com/pUFclTo.png"), updatable;
                    }
                ))
                .non_fungible_data_update_roles(non_fungible_data_update_roles!(
                    non_fungible_data_updater => rule!(require(global_caller(component_address))
                        || require_amount(
                            dec!("0.75"),
                            controller_role.resource_address()
                        ));
                    non_fungible_data_updater_updater => rule!(require_amount(
                        dec!("0.75"),
                        controller_role.resource_address()
                    ));
                ))
                .mint_roles(mint_roles!(
                    minter => rule!(require(global_caller(component_address))
                    || require_amount(
                        dec!("0.75"),
                        controller_role.resource_address()
                    ));
                    minter_updater => rule!(require_amount(
                        dec!("0.75"),
                        controller_role.resource_address()
                    ));
                ))
                .burn_roles(burn_roles!(
                    burner => rule!(require(global_caller(component_address))
                    || require_amount(
                        dec!("0.75"),
                        controller_role.resource_address()
                    ));
                    burner_updater => rule!(require_amount(
                        dec!("0.75"),
                        controller_role.resource_address()
                    ));
                ))
                .create_with_no_initial_supply();

            let cdp_marker_manager: ResourceManager =
                ResourceBuilder::new_integer_non_fungible::<CdpMarker>(OwnerRole::Fixed(rule!(
                    require_amount(dec!("0.75"), controller_role.resource_address())
                )))
                .metadata(metadata!(
                    init {
                        "name" => "Stabilis Marker Receipt", locked;
                        "symbol" => "stabMARK", locked;
                        "description" => "A receipt received by marking a Stabilis loan", updatable;
                        "info_url" => "https://ilikeitstable.com", updatable;
                        "icon_url" => Url::of("https://i.imgur.com/Xi6nrsv.png"), updatable;
                    }
                ))
                .non_fungible_data_update_roles(non_fungible_data_update_roles!(
                    non_fungible_data_updater => rule!(require(global_caller(component_address))
                    || require_amount(
                        dec!("0.75"),
                        controller_role.resource_address()
                    ));
                    non_fungible_data_updater_updater => rule!(require_amount(
                        dec!("0.75"),
                        controller_role.resource_address()
                    ));
                ))
                .mint_roles(mint_roles!(
                    minter => rule!(require(global_caller(component_address))
                    || require_amount(
                        dec!("0.75"),
                        controller_role.resource_address()
                    ));
                    minter_updater => rule!(require_amount(
                        dec!("0.75"),
                        controller_role.resource_address()
                    ));
                ))
                .burn_roles(burn_roles!(
                    burner => rule!(require(global_caller(component_address))
                    || require_amount(
                        dec!("0.75"),
                        controller_role.resource_address()
                    ));
                    burner_updater => rule!(require_amount(
                        dec!("0.75"),
                        controller_role.resource_address()
                    ));
                ))
                .create_with_no_initial_supply();

            let liquidation_receipt_manager: ResourceManager =
                ResourceBuilder::new_integer_non_fungible::<LiquidationReceipt>(OwnerRole::Fixed(
                    rule!(require_amount(
                        dec!("0.75"),
                        controller_role.resource_address()
                    )),
                ))
                .metadata(metadata!(
                    init {
                        "name" => "Stabilis Liquidation Receipt", locked;
                        "symbol" => "stabLIQ", locked;
                        "description" => "A receipt received for liquidating a Stabilis Loan", updatable;
                        "info_url" => "https://ilikeitstable.com", updatable;
                        "icon_url" => Url::of("https://i.imgur.com/UnrCzEM.png"), updatable;
                    }
                ))
                .non_fungible_data_update_roles(non_fungible_data_update_roles!(
                    non_fungible_data_updater => rule!(require(global_caller(component_address))
                    || require_amount(dec!("0.75"),
                    controller_role.resource_address()
                    ));
                    non_fungible_data_updater_updater => rule!(require_amount(
                        dec!("0.75"),
                        controller_role.resource_address()
                    ));
                ))
                .mint_roles(mint_roles!(
                    minter => rule!(require(global_caller(component_address))
                    || require_amount(dec!("0.75"),
                    controller_role.resource_address()
                    ));
                    minter_updater => rule!(require_amount(
                        dec!("0.75"),
                        controller_role.resource_address()
                    ));
                ))
                .burn_roles(burn_roles!(
                    burner => rule!(allow_all);
                    burner_updater => rule!(deny_all);
                ))
                .create_with_no_initial_supply();

            let stabilis = Self {
                collaterals: StabilisKeyValueStore::new_with_registered_type(),
                pool_units: StabilisKeyValueStore::new_with_registered_type(),
                collateral_ratios: StabilisKeyValueStore::new_with_registered_type(),
                cdp_counter: 0,
                cdp_manager,
                stab_manager,
                controller_badge_manager,
                internal_stab_price: dec!(1),
                circulating_stab: dec!(0),
                cdp_marker_manager,
                cdp_marker_counter: 0,
                marked_cdps: AvlTree::new(),
                marked_cdps_active: 0,
                marker_placing_counter: dec!(0),
                liquidation_receipt_manager,
                liquidation_counter: 0,
                parameters,
            }
            .instantiate()
            .prepare_to_globalize(OwnerRole::Fixed(rule!(require_amount(
                dec!("0.75"),
                controller_role.resource_address()
            ))))
            .with_address(address_reservation)
            .globalize();

            (stabilis, controller_role)
        }

        /// Borrow STAB by opening a CDP (taking out a loan vs. collateral)
        ///
        /// # Input
        /// - `collateral`: The collateral to be used
        /// - `stab_to_mint`: The amount of STAB to mint
        /// - `safe`: Whether it is possible to open the loan with less collateral than required (for testing)
        ///
        /// # Output
        /// - The minted STAB in a `Bucket`
        /// - The CDP receipt in a `Bucket`
        ///
        /// # Logic
        /// - Check whether amount to mint > minimum mint and minting is allowed right now
        /// - Check whether collateral is accepted and if it is a pool unit
        /// - Calculate collateral amount, converting pool unit to real (underlying asset) if necessary
        /// - Assign parent address, which is equal to the collateral address unless the collateral is a pool unit
        /// - Check whether collateral value is high enough
        /// - Calculate collateral ratio and insert into AvlTree
        /// - Create CDP struct for the receipt
        /// - Check whether the share of this collateral's minted STAB is too high and update STAB circulating supply
        /// - Mint the CDP receipt
        /// - Store the collateral in the correct vault
        /// - Return the minted STAB and the CDP receipt
        pub fn open_cdp(&mut self, collateral: Bucket, stab_to_mint: Decimal) -> (Bucket, Bucket) {
            let mut is_pool_unit_collateral: bool = false;
            let stab_tokens: Bucket = self.stab_manager.mint(stab_to_mint);

            assert!(
                stab_tokens.amount() >= self.parameters.minimum_mint,
                "Minted STAB is less than the minimum required amount."
            );
            assert!(
                !self.parameters.stop_openings,
                "Not allowed to open loans right now."
            );

            if self
                .pool_units
                .get(&collateral.resource_address())
                .is_some()
            {
                assert!(
                    self.pool_units
                        .get(&collateral.resource_address())
                        .unwrap()
                        .accepted,
                    "This collateral is not accepted"
                );
                is_pool_unit_collateral = true;
            } else {
                assert!(
                    self.collaterals
                        .get(&collateral.resource_address())
                        .map(|c| c.accepted)
                        .unwrap_or(false),
                    "This collateral is not accepted"
                );
            }

            let collateral_amount: Decimal = self.pool_to_real(
                collateral.amount(),
                collateral.resource_address(),
                is_pool_unit_collateral,
            );

            let parent_collateral_address: ResourceAddress = match is_pool_unit_collateral {
                false => collateral.resource_address(),
                true => {
                    self.pool_units
                        .get(&collateral.resource_address())
                        .unwrap()
                        .parent_address
                }
            };

            self.collaterals
                .get_mut(&parent_collateral_address)
                .unwrap()
                .collateral_amount += collateral_amount;

            let mcr: Decimal = self
                .collaterals
                .get(&parent_collateral_address)
                .unwrap()
                .mcr;

            assert!(
                self.collaterals
                    .get(&parent_collateral_address)
                    .unwrap()
                    .usd_price
                    * collateral_amount
                    >= self.internal_stab_price * stab_tokens.amount() * mcr,
                "Collateral value too low."
            );

            self.cdp_counter += 1;

            let cr: Decimal = collateral_amount / stab_tokens.amount();

            if self
                .collaterals
                .get(&parent_collateral_address)
                .unwrap()
                .initialized
            {
                self.insert_cr(
                    parent_collateral_address,
                    cr,
                    NonFungibleLocalId::integer(self.cdp_counter),
                );
            } else {
                let mut avl_tree: AvlTree<Decimal, Vec<NonFungibleLocalId>> = AvlTree::new();
                let cdp_ids: Vec<NonFungibleLocalId> =
                    vec![NonFungibleLocalId::integer(self.cdp_counter)];
                avl_tree.insert(cr, cdp_ids);
                self.collateral_ratios
                    .insert(parent_collateral_address, avl_tree);
                self.collaterals
                    .get_mut(&parent_collateral_address)
                    .unwrap()
                    .initialized = true;
                self.collaterals
                    .get_mut(&parent_collateral_address)
                    .unwrap()
                    .highest_cr = cr;
            }

            let cdp = Cdp {
                collateral: collateral.resource_address(),
                parent_address: parent_collateral_address,
                is_pool_unit_collateral,
                collateral_amount: collateral.amount(),
                minted_stab: stab_tokens.amount(),
                collateral_stab_ratio: cr,
                status: CdpStatus::Healthy,
                marker_id: 0u64,
            };

            self.update_minted_stab(
                true,
                is_pool_unit_collateral,
                true,
                stab_tokens.amount(),
                parent_collateral_address,
                collateral.resource_address(),
            );

            let cdp_receipt: NonFungibleBucket = self
                .cdp_manager
                .mint_non_fungible(&NonFungibleLocalId::integer(self.cdp_counter), cdp)
                .as_non_fungible();

            self.put_collateral(
                collateral.resource_address(),
                is_pool_unit_collateral,
                collateral,
            );

            (stab_tokens, cdp_receipt.into())
        }

        ///Close a loan / CDP, by paying off the debt
        ///
        /// # Input
        /// - `receipt_id`: The CDP receipt
        /// - `stab_payment`: The STAB tokens to pay back
        ///
        /// # Output
        /// - The collateral returned
        /// - The leftover STAB
        ///
        /// # Logic
        /// - Check if the STAB payment is enough to close the loan
        /// - Check if the loan is healthy
        /// - Check if the STAB payment is valid
        /// - Remove collateral from the vault
        /// - Update circulating STAB, both for total and chosen collateral
        /// - Burn the paid back STAB
        /// - Remove the collateral ratio from the AvlTree
        /// - Update the CDP receipt
        /// - Return the collateral and the leftover STAB
        pub fn close_cdp(
            &mut self,
            receipt_id: NonFungibleLocalId,
            mut stab_payment: Bucket,
        ) -> (Bucket, Bucket) {
            let receipt_data: Cdp = self.cdp_manager.get_non_fungible_data(&receipt_id);

            assert!(
                stab_payment.amount() >= receipt_data.minted_stab,
                "not enough STAB supplied to close completely"
            );
            assert!(
                !self.parameters.stop_closings,
                "Not allowed to close loans right now."
            );
            assert!(
                receipt_data.status == CdpStatus::Healthy,
                "Loan not healthy. Can't close right now. In case of liquidation, retrieve collateral. Else, add collateral to save."
            );
            assert!(
                stab_payment.resource_address() == self.stab_manager.address(),
                "Invalid STAB payment."
            );

            let collateral: Bucket = self.take_collateral(
                receipt_data.collateral,
                receipt_data.is_pool_unit_collateral,
                receipt_data.collateral_amount,
            );

            self.collaterals
                .get_mut(&receipt_data.parent_address)
                .unwrap()
                .collateral_amount -= receipt_data.collateral_stab_ratio * receipt_data.minted_stab;

            self.update_minted_stab(
                false,
                receipt_data.is_pool_unit_collateral,
                false,
                receipt_data.minted_stab,
                receipt_data.parent_address,
                receipt_data.collateral,
            );

            stab_payment.take(receipt_data.minted_stab).burn();

            self.remove_cr(
                receipt_data.parent_address,
                receipt_data.collateral_stab_ratio,
                receipt_id.clone(),
            );

            self.cdp_manager
                .update_non_fungible_data(&receipt_id, "status", CdpStatus::Closed);

            self.cdp_manager
                .update_non_fungible_data(&receipt_id, "collateral_amount", dec!(0));

            (collateral, stab_payment)
        }

        /// Retrieve leftover collateral from a liquidated loan / cdp
        ///
        /// # Input
        /// - `receipt_id`: The CDP receipt
        ///
        /// # Output
        /// - The leftover collateral
        ///
        /// # Logic
        /// - Check if the loan is liquidated
        /// - Check if there is leftover collateral
        /// - Check if it is allowed to close loans right now
        /// - Update CDP receipt to 0 collateral
        /// - Return the leftover collateral
        pub fn retrieve_leftover_collateral(&mut self, receipt_id: NonFungibleLocalId) -> Bucket {
            let receipt_data: Cdp = self.cdp_manager.get_non_fungible_data(&receipt_id);

            assert!(
                receipt_data.status == CdpStatus::Liquidated
                    || receipt_data.status == CdpStatus::ForceLiquidated,
                "Loan not liquidated"
            );
            assert!(
                receipt_data.collateral_amount > dec!(0),
                "No collateral leftover"
            );
            assert!(
                !self.parameters.stop_closings,
                "Not allowed to close loans right now."
            );

            self.cdp_manager
                .update_non_fungible_data(&receipt_id, "collateral_amount", dec!(0));

            self.take_collateral(
                receipt_data.collateral,
                receipt_data.is_pool_unit_collateral,
                receipt_data.collateral_amount,
            )
        }

        /// Add collateral to a loan / CDP
        ///
        /// # Input
        /// - `collateral_id`: The CDP receipt
        /// - `collateral`: The collateral to add
        ///
        /// # Output
        /// - None
        ///
        /// # Logic
        /// - Check if the loan is healthy or marked
        /// - Check if the collateral is compatible
        /// - Remove the collateral ratio from the AvlTree
        /// - Calculate new collateral ratio
        /// - Update the collateral amount in the CDP receipt
        /// - Check if the new collateral ratio is high enough
        /// - Insert new collateral ratio into AvlTree
        /// - Store the collateral in the correct vault
        /// - Update the CDP receipt
        /// - If the loan was marked, update the marker receipt
        pub fn top_up_cdp(&mut self, collateral_id: NonFungibleLocalId, collateral: Bucket) {
            let receipt_data: Cdp = self.cdp_manager.get_non_fungible_data(&collateral_id);
            let new_collateral_amount = receipt_data.collateral_amount + collateral.amount();

            assert!(
                receipt_data.status == CdpStatus::Healthy
                    || receipt_data.status == CdpStatus::Marked,
                "Loan not healthy or marked."
            );
            assert!(
                receipt_data.collateral == collateral.resource_address(),
                "Incompatible token."
            );

            if receipt_data.status == CdpStatus::Healthy {
                self.remove_cr(
                    receipt_data.parent_address,
                    receipt_data.collateral_stab_ratio,
                    collateral_id.clone(),
                );
            }

            let cr: Decimal = self.pool_to_real(
                new_collateral_amount,
                collateral.resource_address(),
                receipt_data.is_pool_unit_collateral,
            ) / receipt_data.minted_stab;

            self.collaterals
                .get_mut(&receipt_data.parent_address)
                .unwrap()
                .collateral_amount +=
                (cr - receipt_data.collateral_stab_ratio) * receipt_data.minted_stab;

            assert!(
                cr > self
                    .collaterals
                    .get(&receipt_data.parent_address)
                    .unwrap()
                    .liquidation_collateral_ratio,
                "Not enough collateral added to save this loan."
            );

            self.insert_cr(receipt_data.parent_address, cr, collateral_id.clone());

            self.put_collateral(
                receipt_data.collateral,
                receipt_data.is_pool_unit_collateral,
                collateral,
            );

            self.cdp_manager
                .update_non_fungible_data(&collateral_id, "collateral_stab_ratio", cr);
            self.cdp_manager.update_non_fungible_data(
                &collateral_id,
                "collateral_amount",
                new_collateral_amount,
            );

            if receipt_data.status == CdpStatus::Marked {
                let marker_data: CdpMarker = self
                    .cdp_marker_manager
                    .get_non_fungible_data(&NonFungibleLocalId::integer(receipt_data.marker_id));
                self.cdp_manager.update_non_fungible_data(
                    &collateral_id,
                    "status",
                    CdpStatus::Healthy,
                );
                self.cdp_marker_manager.update_non_fungible_data(
                    &NonFungibleLocalId::integer(receipt_data.marker_id),
                    "used",
                    true,
                );
                self.marked_cdps.remove(&marker_data.marker_placing);
                self.marked_cdps_active -= 1;
            }
        }

        /// Remove collateral from a loan / CDP
        ///
        /// # Input
        /// - `collateral_id`: The CDP receipt
        /// - `amount`: The amount of collateral to remove
        ///
        /// # Output
        /// - The removed collateral
        ///
        /// # Logic
        /// - Check if the loan is healthy
        /// - Remove the collateral ratio from the AvlTree
        /// - Calculate new collateral ratio
        /// - Check if the new collateral ratio is high enough
        /// - Insert new collateral ratio into AvlTree
        /// - Retrieve the to-be returned collateral from the correct vault
        /// - Update the CDP receipt
        /// - Return the removed collateral
        pub fn remove_collateral(
            &mut self,
            collateral_id: NonFungibleLocalId,
            amount: Decimal,
        ) -> Bucket {
            let receipt_data: Cdp = self.cdp_manager.get_non_fungible_data(&collateral_id);
            let new_collateral_amount = receipt_data.collateral_amount - amount;

            assert!(
                receipt_data.status == CdpStatus::Healthy,
                "Loan not healthy. Save it first."
            );

            assert!(
                !self.parameters.stop_closings,
                "Not allowed to close loans / remove collateral right now."
            );

            self.remove_cr(
                receipt_data.parent_address,
                receipt_data.collateral_stab_ratio,
                collateral_id.clone(),
            );

            let cr: Decimal = self.pool_to_real(
                new_collateral_amount,
                receipt_data.collateral,
                receipt_data.is_pool_unit_collateral,
            ) / receipt_data.minted_stab;

            self.collaterals
                .get_mut(&receipt_data.parent_address)
                .unwrap()
                .collateral_amount +=
                (cr - receipt_data.collateral_stab_ratio) * receipt_data.minted_stab;

            self.insert_cr(receipt_data.parent_address, cr, collateral_id.clone());

            assert!(
                cr > self
                    .collaterals
                    .get_mut(&receipt_data.parent_address)
                    .unwrap()
                    .liquidation_collateral_ratio,
                "Removal would put the CR below MCR."
            );

            let removed_collateral: Bucket = self.take_collateral(
                receipt_data.collateral,
                receipt_data.is_pool_unit_collateral,
                amount,
            );

            self.cdp_manager
                .update_non_fungible_data(&collateral_id, "collateral_stab_ratio", cr);
            self.cdp_manager.update_non_fungible_data(
                &collateral_id,
                "collateral_amount",
                new_collateral_amount,
            );

            removed_collateral
        }

        /// Partially close a loan / CDP (pay off part of the debt)
        ///
        /// # Input
        /// - `collateral_id`: The CDP receipt
        /// - `repayment`: The STAB tokens to pay back
        ///
        /// # Output
        /// - None
        ///
        /// # Logic
        /// - Check if the STAB payment is valid
        /// - If the repayment > debt, close the loan and return leftover collateral and leftover payment
        /// - Check if borrowed amount is still above minimum borrow
        /// - Check if the loan is healthy or marked
        /// - Remove the collateral ratio from the AvlTree if not marked
        /// - Calculate new collateral ratio
        /// - Burn the paid back STAB
        /// - Insert new collateral ratio into AvlTree
        /// - Check if the new collateral ratio is high enough
        /// - Update the CDP receipt
        /// - If the loan was marked, update the marker receipt
        pub fn partial_close_cdp(
            &mut self,
            collateral_id: NonFungibleLocalId,
            repayment: Bucket,
        ) -> (Option<Bucket>, Option<Bucket>) {
            assert!(
                !self.parameters.stop_closings,
                "Not allowed to close loans / remove collateral right now."
            );

            assert!(
                repayment.resource_address() == self.stab_manager.address(),
                "Invalid STAB payment."
            );

            let receipt_data: Cdp = self.cdp_manager.get_non_fungible_data(&collateral_id);
            let new_stab_amount = receipt_data.minted_stab - repayment.amount();

            if new_stab_amount < dec!(0) {
                let (collateral, leftover_payment): (Bucket, Bucket) =
                    self.close_cdp(collateral_id, repayment);
                return (Some(collateral), Some(leftover_payment));
            }

            assert!(
                new_stab_amount >= self.parameters.minimum_mint,
                "Resulting borrowed STAB needs to be above minimum mint."
            );

            assert!(
                receipt_data.status == CdpStatus::Healthy
                    || receipt_data.status == CdpStatus::Marked,
                "Loan not healthy or marked."
            );

            if receipt_data.status == CdpStatus::Healthy {
                self.remove_cr(
                    receipt_data.parent_address,
                    receipt_data.collateral_stab_ratio,
                    collateral_id.clone(),
                );
            }

            let cr: Decimal = self.pool_to_real(
                receipt_data.collateral_amount,
                receipt_data.collateral,
                receipt_data.is_pool_unit_collateral,
            ) / new_stab_amount;

            self.update_minted_stab(
                false,
                receipt_data.is_pool_unit_collateral,
                false,
                repayment.amount(),
                receipt_data.parent_address,
                receipt_data.collateral,
            );

            repayment.burn();

            self.insert_cr(receipt_data.parent_address, cr, collateral_id.clone());

            assert!(
                cr > self
                    .collaterals
                    .get_mut(&receipt_data.parent_address)
                    .unwrap()
                    .liquidation_collateral_ratio,
                "CR below MCR."
            );

            self.cdp_manager
                .update_non_fungible_data(&collateral_id, "collateral_stab_ratio", cr);
            self.cdp_manager.update_non_fungible_data(
                &collateral_id,
                "minted_stab",
                new_stab_amount,
            );

            if receipt_data.status == CdpStatus::Marked {
                let marker_data: CdpMarker = self
                    .cdp_marker_manager
                    .get_non_fungible_data(&NonFungibleLocalId::integer(receipt_data.marker_id));
                self.cdp_manager.update_non_fungible_data(
                    &collateral_id,
                    "status",
                    CdpStatus::Healthy,
                );
                self.cdp_marker_manager.update_non_fungible_data(
                    &NonFungibleLocalId::integer(receipt_data.marker_id),
                    "used",
                    true,
                );
                self.marked_cdps.remove(&marker_data.marker_placing);
                self.marked_cdps_active -= 1;
            }

            (None, None)
        }

        /// Borrow more STAB by adding to the loan / CDP
        ///
        /// # Input
        /// - `collateral_id`: The CDP receipt
        /// - `amount`: The amount of STAB to mint / borrow
        ///
        /// # Output
        /// - The minted STAB in a `Bucket`
        ///
        /// # Logic
        /// - Check if the loan is healthy
        /// - Remove the collateral ratio from the AvlTree
        /// - Calculate new collateral ratio
        /// - Update the minted STAB
        /// - Insert new collateral ratio into AvlTree
        /// - Check if the new collateral ratio is high enough
        /// - Update the CDP receipt
        /// - Mint the STAB and return it
        pub fn borrow_more(
            &mut self,
            collateral_id: NonFungibleLocalId,
            amount: Decimal,
        ) -> Bucket {
            let receipt_data: Cdp = self.cdp_manager.get_non_fungible_data(&collateral_id);
            let new_stab_amount = receipt_data.minted_stab + amount;

            assert!(
                receipt_data.status == CdpStatus::Healthy,
                "Loan not healthy. Save it first."
            );

            assert!(
                !self.parameters.stop_openings,
                "Not allowed to open loans right now."
            );

            self.remove_cr(
                receipt_data.parent_address,
                receipt_data.collateral_stab_ratio,
                collateral_id.clone(),
            );

            let cr: Decimal = self.pool_to_real(
                receipt_data.collateral_amount,
                receipt_data.collateral,
                receipt_data.is_pool_unit_collateral,
            ) / new_stab_amount;

            self.update_minted_stab(
                true,
                receipt_data.is_pool_unit_collateral,
                true,
                amount,
                receipt_data.parent_address,
                receipt_data.collateral,
            );

            self.insert_cr(receipt_data.parent_address, cr, collateral_id.clone());

            assert!(
                cr > self
                    .collaterals
                    .get_mut(&receipt_data.parent_address)
                    .unwrap()
                    .liquidation_collateral_ratio,
                "Removal would put the CR below MCR."
            );

            self.cdp_manager
                .update_non_fungible_data(&collateral_id, "collateral_stab_ratio", cr);
            self.cdp_manager.update_non_fungible_data(
                &collateral_id,
                "minted_stab",
                new_stab_amount,
            );

            self.stab_manager.mint(amount)
        }

        /// Mark a loan for liquidation
        ///
        /// # Input
        /// - `collateral`: The collateral for which to look for undercollateralized loans to be liquidated
        ///
        /// # Output
        /// - The marker receipt in a `Bucket`
        ///
        /// # Logic
        /// - Get the CDP with the lowest collateral ratio for the chosen collateral
        /// - Calculate new collateral ratio (as pool unit aren't always up to date)
        /// - Create the marker receipt struct
        /// - Insert the CDP into the marked CDPs AvlTree
        /// - Remove the collateral ratio from the AvlTree
        /// - Mint marker receipt, which will be returned if the marking is a success
        /// - Update the Cdp receipt to point to the marker receipt and get marked status
        /// - Get the collateral ids for the collateral ratio that was newly calculated (which is different if working with pool units)
        /// - Save CDP if CR is high enough after pool_to_real conversion, unless the collateral_ids vector is full
        ///     - Return the initial marker receipt if saving wasn't possible
        ///     - Or return a new marker receipt if saving was possible
        pub fn mark_for_liquidation(&mut self, collateral: ResourceAddress) -> Bucket {
            let (_first_cr, collateral_ids, _next_key) = self
                .collateral_ratios
                .get_mut(&collateral)
                .unwrap()
                .range(dec!(0)..)
                .next()
                .unwrap();
            let collateral_id: NonFungibleLocalId = collateral_ids[0].clone();
            let lcr: Decimal = self
                .collaterals
                .get(&collateral)
                .unwrap()
                .liquidation_collateral_ratio;
            let data: Cdp = self.cdp_manager.get_non_fungible_data(&collateral_id);

            assert!(
                data.collateral_stab_ratio
                    < self
                        .collaterals
                        .get(&collateral)
                        .unwrap()
                        .liquidation_collateral_ratio,
                "No possible liquidations."
            );

            let cr: Decimal = self.pool_to_real(
                data.collateral_amount,
                data.collateral,
                data.is_pool_unit_collateral,
            ) / data.minted_stab;

            self.collaterals
                .get_mut(&collateral)
                .unwrap()
                .collateral_amount += (cr - data.collateral_stab_ratio) * data.minted_stab;

            self.marker_placing_counter += dec!(1);
            self.cdp_marker_counter += 1;
            let id: Decimal = self.marker_placing_counter;

            let mut marker = CdpMarker {
                mark_type: CdpUpdate::Marked,
                time_marked: Clock::current_time_rounded_to_seconds(),
                marked_id: collateral_id.clone(),
                marker_placing: self.marker_placing_counter,
                used: false,
            };

            self.remove_cr(
                data.parent_address,
                data.collateral_stab_ratio,
                collateral_id.clone(),
            );

            let mut cdp_ids: Vec<NonFungibleLocalId> = Vec::new();

            if self
                .collateral_ratios
                .get_mut(&collateral)
                .unwrap()
                .get_mut(&cr)
                .is_some()
            {
                cdp_ids = self
                    .collateral_ratios
                    .get_mut(&collateral)
                    .unwrap()
                    .get_mut(&cr)
                    .unwrap()
                    .to_vec();
            }

            if (cr > lcr) && cdp_ids.len() < self.parameters.max_vector_length.try_into().unwrap() {
                self.insert_cr(data.parent_address, cr, collateral_id.clone());

                self.cdp_manager.update_non_fungible_data(
                    &collateral_id,
                    "collateral_stab_ratio",
                    cr,
                );

                marker.mark_type = CdpUpdate::Saved;

                self.cdp_marker_manager.mint_non_fungible(
                    &NonFungibleLocalId::integer(self.cdp_marker_counter),
                    marker,
                )
            } else {
                self.marked_cdps.insert(id, collateral_id.clone());
                self.marked_cdps_active += 1;

                self.cdp_manager.update_non_fungible_data(
                    &collateral_id,
                    "marker_id",
                    self.cdp_marker_counter,
                );

                self.cdp_manager.update_non_fungible_data(
                    &collateral_id,
                    "status",
                    CdpStatus::Marked,
                );

                self.cdp_marker_manager.mint_non_fungible(
                    &NonFungibleLocalId::integer(self.cdp_marker_counter),
                    marker,
                )
            }
        }

        /// Force liquidate a loan / CDP (liquidating without the loan being undercollateralized, but with a fee that should be beneficial for the borrower)
        ///
        /// # Input
        /// - `collateral`: The collateral to be liquidated
        /// - `payment`: The STAB tokens to pay back
        /// - `percentage_to_take`: The percentage of the collateral value to take (if < 1, the borrower will profit off the liquidation)
        /// - `assert_non_markable`: Whether to assert that the loan is not markable via normal means, which would be more profitable for the liquidator
        ///
        /// # Output
        /// - The collateral returned
        /// - The leftover STAB
        ///
        /// # Logic
        /// - Get the CDP with lowest collateral ratio for the chosen collateral
        /// - Remove the collateral ratio from the AvlTree
        /// - Calculate latest collateral ratio
        /// - Get liquidation collateral ratio
        /// - Assert that the collateral ratio is high enough to force liquidate
        ///    - If LCR > CR, the loan doesn't have to be forced, but can be liquidated via normal means
        /// - Calculate percentage of collateral value vs. minted STAB
        /// - Calculate how much of the loan can be liquidated, how much of the payment should be taken, and what the leftover STAB debt will then be
        /// - Calculate new collateral amount
        ///    - If CR is too low, not all collateral the liquidator wants to take can be taken, so the new_collateral_amount will be negative
        ///    - Then we set the new_collateral_amount to 0
        /// - If CR percentage is not > 100%, the entire loan must be liquidated
        ///    - Otherwise we can be left with a loan that has debt but 0 collateral
        /// - Take the payment and burn the STAB
        /// - Update circulating STAB
        /// - Take the collateral
        /// - Calculate the new_collateral_amount again, because of potential rounding in take_collateral method (when working with assets with strange divisilibity)
        /// - Update the CDP receipt
        /// - If the new collateral amount is not 0, calculate the new collateral ratio, insert it into the AvlTree and update the CDP receipt
        /// - If the loan was liquidated, update the CDP receipt to reflect this
        /// - Return the collateral and the leftover STAB
        pub fn force_liquidate(
            &mut self,
            collateral: ResourceAddress,
            mut payment: Bucket,
            percentage_to_take: Decimal,
            assert_non_markable: bool,
        ) -> (Bucket, Bucket) {
            assert!(
                !self.parameters.stop_force_liquidate,
                "Not allowed to forceliquidate loans right now."
            );

            assert!(
                payment.resource_address() == self.stab_manager.address(),
                "Invalid STAB payment."
            );

            let (_first_cr, collateral_ids, _next_key) = self
                .collateral_ratios
                .get_mut(&collateral)
                .unwrap()
                .range(dec!(0)..)
                .next()
                .unwrap();
            let collateral_id: NonFungibleLocalId = collateral_ids[0].clone();
            let data: Cdp = self.cdp_manager.get_non_fungible_data(&collateral_id);

            self.remove_cr(
                data.parent_address,
                data.collateral_stab_ratio,
                collateral_id.clone(),
            );

            let cr: Decimal = self.pool_to_real(
                data.collateral_amount,
                data.collateral,
                data.is_pool_unit_collateral,
            ) / data.minted_stab;

            let lcr: Decimal = self
                .collaterals
                .get(&collateral)
                .unwrap()
                .liquidation_collateral_ratio;

            if assert_non_markable {
                assert!(
                    cr > lcr,
                    "CR is too low. Liquidate this loan via the normal procedure."
                );
            }

            let cr_percentage: Decimal = self.collaterals.get(&collateral).unwrap().mcr * cr / lcr;

            let (percentage_to_liquidate, payment_amount, new_stab_amount): (
                Decimal,
                Decimal,
                Decimal,
            ) = match payment.amount() > data.minted_stab {
                true => (dec!(1), data.minted_stab, dec!(0)),
                false => (
                    (payment.amount() / data.minted_stab),
                    payment.amount(),
                    data.minted_stab - payment.amount(),
                ),
            };

            let mut new_collateral_amount: Decimal = data.collateral_amount
                - (data.collateral_amount * percentage_to_liquidate * percentage_to_take
                    / cr_percentage);

            if new_collateral_amount < dec!(0) {
                new_collateral_amount = dec!(0);
            }

            assert!(
                cr_percentage > dec!(1) || percentage_to_liquidate == dec!(1),
                "CR < 100%. Entire loan must be liquidated",
            );

            payment.take(payment_amount).burn();

            self.update_minted_stab(
                false,
                data.is_pool_unit_collateral,
                false,
                payment_amount,
                data.parent_address,
                data.collateral,
            );

            let collateral_payment: Bucket = self.take_collateral(
                data.collateral,
                data.is_pool_unit_collateral,
                data.collateral_amount - new_collateral_amount,
            );

            new_collateral_amount = data.collateral_amount - collateral_payment.amount();

            self.cdp_manager.update_non_fungible_data(
                &collateral_id,
                "collateral_amount",
                new_collateral_amount,
            );
            self.cdp_manager.update_non_fungible_data(
                &collateral_id,
                "minted_stab",
                new_stab_amount,
            );

            if percentage_to_liquidate < dec!(1) {
                let new_cr: Decimal = self.pool_to_real(
                    new_collateral_amount,
                    data.collateral,
                    data.is_pool_unit_collateral,
                ) / new_stab_amount;

                self.collaterals
                    .get_mut(&data.parent_address)
                    .unwrap()
                    .collateral_amount += (new_cr - data.collateral_stab_ratio) * data.minted_stab;

                self.insert_cr(data.parent_address, new_cr, collateral_id.clone());

                self.cdp_manager.update_non_fungible_data(
                    &collateral_id,
                    "collateral_stab_ratio",
                    new_cr,
                );
            } else {
                self.cdp_manager.update_non_fungible_data(
                    &collateral_id,
                    "status",
                    CdpStatus::ForceLiquidated,
                );

                self.collaterals
                    .get_mut(&data.parent_address)
                    .unwrap()
                    .collateral_amount -= data.collateral_stab_ratio * data.minted_stab;
            }

            (collateral_payment, payment)
        }

        /// Force mint STAB by adding collateral to a loan / CDP
        ///
        /// # Input
        /// - `collateral`: The collateral to add
        /// - `payment`: The STAB tokens to pay back
        /// - `percentage_to_supply`: The percentage of the collateral value to supply (if > 1, the borrower will profit off the minting)
        ///
        /// # Output
        /// - The minted STAB in a `Bucket`
        /// - The leftover collateral in a `Bucket`
        ///
        /// # Logic
        /// - Check if it is allowed to force mint right now
        /// - Get the CDP with highest collateral ratio for the chosen collateral
        /// - Check if the collateral is compatible
        /// - Calculate minimum allowed collateral ratio
        /// - Get collateral price
        /// - Calculate constant k, which is the collateral needed for minting 1 STAB
        /// - Calculate the max addition of collateral that can be supplied (see code for calculation and explanation)
        /// - If too much collateral is supplied, remove the excess and put in bucket to return (handle potential rounding errors for strange divisilibity assets)
        /// - Remove the current collateral ratio from the AvlTree
        /// - Calculate newly minted STAB, new collateral amount and new collateral ratio
        /// - Update circulating STAB
        /// - Update the CDP receipt
        /// - Insert the new collateral ratio into the AvlTree
        /// - Mint the STAB
        /// - Return the minted STAB and the leftover collateral
        pub fn force_mint(
            &mut self,
            collateral: ResourceAddress,
            mut payment: Bucket,
            percentage_to_supply: Decimal,
        ) -> (Bucket, Option<Bucket>) {
            assert!(
                !self.parameters.stop_force_mint,
                "Not allowed to force mint right now."
            );

            let mut data: Option<Cdp> = None;
            let mut collateral_id: NonFungibleLocalId = NonFungibleLocalId::integer(0);
            let mut return_bucket: Option<Bucket> = None;

            {
                let collateral_ratios = self.collateral_ratios.get_mut(&collateral).unwrap();
                let range = collateral_ratios.range_back(
                    dec!(0)..(self.collaterals.get(&collateral).unwrap().highest_cr + dec!(1)),
                );

                'outer_loop: for (_cr, collateral_ids, _next_key) in range {
                    for found_collateral_id in collateral_ids {
                        data = Some(self.cdp_manager.get_non_fungible_data(&found_collateral_id));
                        if data.as_ref().unwrap().collateral == payment.resource_address() {
                            collateral_id = found_collateral_id.clone();
                            break 'outer_loop;
                        }
                    }
                }
            }

            let data = data.expect("No suitable mints found");
            assert!(
                data.collateral == payment.resource_address(),
                "Can only force mint other collaterals right now."
            );

            let pool_to_real: Decimal =
                self.pool_to_real(dec!(1), data.collateral, data.is_pool_unit_collateral);

            let min_collateral_ratio: Decimal = self.parameters.force_mint_cr_multiplier
                * self
                    .collaterals
                    .get(&collateral)
                    .unwrap()
                    .liquidation_collateral_ratio;

            let collateral_price: Decimal = self.collaterals.get(&collateral).unwrap().usd_price;

            let k: Decimal = (self.internal_stab_price) / (pool_to_real * collateral_price)
                * percentage_to_supply;

            //we now need to calculate maximum amount of collateral that can be supplied: max_addition
            //we can do this by first claiming: collateral_amount / stab_amount = min_collateral_ratio (1)
            //collateral_amount = (initial_collateral_amount + max_col_addition) * pool_to_real (2)
            //stab_amount = initial_stab_amount + max_col_addition / k (3)
            //filling in (2) and (3) in (1) gives us an equation of the form: ((c + a) * p) / (s + a / k) = m (4)
            //solving (4) for max_col_addition (abbreviated 'a') gives: a = (k * (c * p - m * s)) / (m - k * p)
            //which translates to:

            let max_addition: Decimal = (k
                * (data.collateral_amount * pool_to_real
                    - min_collateral_ratio * data.minted_stab))
                / (min_collateral_ratio - k * pool_to_real);

            if payment.amount() > max_addition {
                return_bucket = Some(payment.take_advanced(
                    payment.amount() - max_addition,
                    WithdrawStrategy::Rounded(RoundingMode::AwayFromZero),
                ));
            }

            self.remove_cr(
                data.parent_address,
                data.collateral_stab_ratio,
                collateral_id.clone(),
            );

            let new_minted_stab: Decimal = data.minted_stab + payment.amount() / k;
            let new_collateral_amount: Decimal = data.collateral_amount + payment.amount();

            let new_cr: Decimal = self.pool_to_real(
                new_collateral_amount,
                data.collateral,
                data.is_pool_unit_collateral,
            ) / new_minted_stab;

            self.collaterals
                .get_mut(&data.parent_address)
                .unwrap()
                .collateral_amount += (new_cr - data.collateral_stab_ratio) * data.minted_stab;

            self.cdp_manager.update_non_fungible_data(
                &collateral_id,
                "minted_stab",
                new_minted_stab,
            );
            self.cdp_manager.update_non_fungible_data(
                &collateral_id,
                "collateral_amount",
                new_collateral_amount,
            );
            self.cdp_manager.update_non_fungible_data(
                &collateral_id,
                "collateral_stab_ratio",
                new_cr,
            );

            self.insert_cr(data.parent_address, new_cr, collateral_id.clone());

            let stab_tokens: Bucket = self.stab_manager.mint(payment.amount() / k);

            self.update_minted_stab(
                false,
                data.is_pool_unit_collateral,
                false,
                stab_tokens.amount(),
                data.parent_address,
                data.collateral,
            );

            self.put_collateral(data.collateral, data.is_pool_unit_collateral, payment);

            (stab_tokens, return_bucket)
        }

        /// Liquidate a marked loan / CDP, using a marker receipt
        ///
        /// # Input
        /// - `marker_id`: The marker receipt id
        /// - `payment`: The STAB tokens to pay back
        ///
        /// # Output, depends on outcome:
        /// 1: liquidation successful
        /// - The collateral reward
        /// - The leftover STAB
        /// - A liquidation receipt
        /// 2: liquidation unsuccessful (because the loan was saved):
        /// - The STAB tokens
        /// - None
        /// - A liquidation receipt (with saved status)
        ///
        /// # Logic
        /// - Get the marker receipt and data
        /// - Get the CDP data according to the marker receipt
        /// - Try to liquidate the CDP, using the try_liquidate method (see that method for more details)
        pub fn liquidate_position_with_marker(
            &mut self,
            marker_id: NonFungibleLocalId,
            payment: Bucket,
        ) -> (Option<Bucket>, Option<Bucket>, Bucket) {
            assert!(
                payment.resource_address() == self.stab_manager.address(),
                "Invalid STAB payment."
            );
            let marker_data: CdpMarker = self.cdp_marker_manager.get_non_fungible_data(&marker_id);

            let cdp_data: Cdp = self
                .cdp_manager
                .get_non_fungible_data(&marker_data.marked_id);

            self.try_liquidate(
                payment,
                cdp_data,
                marker_data,
                marker_id,
                self.parameters.liquidation_delay,
            )
        }

        /// Liquidate a marked loan / CDP, without a marker receipt
        ///
        /// # Input
        /// - `marker_id`: The marker receipt id
        /// - `payment`: The STAB tokens to pay back
        ///
        /// # Output, depends on outcome:
        /// 1: liquidation successful
        /// - The collateral reward
        /// - The leftover STAB
        /// - A liquidation receipt
        /// 2: liquidation unsuccessful (because the loan was saved):
        /// - The STAB tokens
        /// - None
        /// - A liquidation receipt (with saved status)
        ///
        /// # Logic
        /// - Get the CDP to be liquidated:
        ///   - If automatic is true, find the next to-be liquidated CDP, skipping over the amount of CDPs specified by the skip parameter.
        ///     - If no CDPs are found, panic.
        ///     - If too many CDPs are skipped, panic.
        ///   - If automatic is false, use the CDP receipt specified by the cdp_id parameter.
        /// - Get the marker receipt through the CDP data
        /// - Try to liquidate the CDP, using the try_liquidate method (see that method for more details)
        pub fn liquidate_position_without_marker(
            &mut self,
            payment: Bucket,
            skip: Option<i64>,
            cdp_id: NonFungibleLocalId,
        ) -> (Option<Bucket>, Option<Bucket>, Bucket) {
            assert!(
                payment.resource_address() == self.stab_manager.address(),
                "Invalid STAB payment."
            );
            let mut collateral_id: NonFungibleLocalId = cdp_id;
            let mut skip_counter: i64 = 0;
            let mut found: bool = false;

            if let Some(skip) = skip {
                for (_identifier, found_collateral_id, _next_key) in
                    self.marked_cdps.range(dec!(0)..)
                {
                    collateral_id = found_collateral_id.clone();
                    skip_counter += 1;
                    if (skip_counter - 1) == skip {
                        found = true;
                        break;
                    }
                }
                if skip_counter == 0 {
                    panic!("No loans available to liquidate.");
                } else if !found {
                    panic!(
                        "Too many skipped. Skip a maximum of {} loans.",
                        skip_counter - 1
                    );
                }
            }

            let cdp_data: Cdp = self.cdp_manager.get_non_fungible_data(&collateral_id);
            let marker_data: CdpMarker = self
                .cdp_marker_manager
                .get_non_fungible_data(&NonFungibleLocalId::integer(cdp_data.marker_id));

            let marker_id: NonFungibleLocalId = NonFungibleLocalId::integer(cdp_data.marker_id);

            self.try_liquidate(
                payment,
                cdp_data,
                marker_data,
                marker_id,
                self.parameters.liquidation_delay + self.parameters.unmarked_delay,
            )
        }

        /// Changes the price of a collateral, which will also update the liquidation collateral ratio
        pub fn change_collateral_price(&mut self, collateral: ResourceAddress, new_price: Decimal) {
            let mcr: Decimal = self.collaterals.get_mut(&collateral).unwrap().mcr;
            self.collaterals.get_mut(&collateral).unwrap().usd_price = new_price;
            self.collaterals
                .get_mut(&collateral)
                .unwrap()
                .liquidation_collateral_ratio = mcr * (self.internal_stab_price / new_price);
        }

        /// Add a possible collateral to the protocol
        pub fn add_collateral(
            &mut self,
            address: ResourceAddress,
            chosen_mcr: Decimal,
            initial_price: Decimal,
        ) {
            assert!(
                self.collaterals.get(&address).is_none(),
                "Collateral is already accepted."
            );

            let info = CollateralInfo {
                mcr: chosen_mcr,
                usd_price: initial_price,
                liquidation_collateral_ratio: chosen_mcr * self.internal_stab_price / initial_price,
                vault: Vault::new(address),
                resource_address: address,
                treasury: Vault::new(address),
                accepted: true,
                initialized: false,
                max_stab_share: dec!(1),
                minted_stab: dec!(0),
                collateral_amount: dec!(0),
                highest_cr: dec!(0),
            };

            self.collaterals.insert(address, info);
        }

        /// Add a possible pool collateral to the protocol
        ///   - pool collaterals are collaterals are some kind of pool unit (such as LSUs), with an underlying asset that is already a collateral
        ///       - the collateral amount is calculated when a loan is opened and interacted with, so not continuously updated
        ///          - this means that sometimes a loan can be liquidated, but when interacting with it, the collateral amount is updated so it can't be anymore
        ///             - this results in the loan being saved
        pub fn add_pool_collateral(
            &self,
            address: ResourceAddress,
            parent_address: ResourceAddress,
            pool_address: ComponentAddress,
            lsu: bool,
            initial_acceptance: bool,
        ) {
            assert!(
                self.pool_units.get(&address).is_none(),
                "Collateral is already accepted."
            );

            let mut validator: Option<Global<Validator>> = None;
            let mut one_resource_pool: Option<Global<OneResourcePool>> = None;

            if lsu {
                validator = Some(Global::from(pool_address));
            } else {
                one_resource_pool = Some(Global::from(pool_address));
            }

            let info = PoolUnitInfo {
                vault: Vault::new(address),
                treasury: Vault::new(address),
                lsu,
                validator,
                one_resource_pool,
                parent_address,
                address,
                accepted: initial_acceptance,
                max_pool_share: dec!(1),
                minted_stab: dec!(0),
            };

            self.pool_units.insert(address, info);
        }

        /// Changes the internal price of the STAB token
        pub fn change_internal_price(&mut self, new_price: Decimal) {
            self.internal_stab_price = new_price;
        }

        ///Emptying the treasury of a collateral, error_fallback exists if a pool unit is also in self.collaterals
        pub fn empty_collateral_treasury(
            &mut self,
            amount: Decimal,
            collateral: ResourceAddress,
            error_fallback: bool,
        ) -> Bucket {
            if self.pool_units.get(&collateral).is_some() && !error_fallback {
                return self
                    .pool_units
                    .get_mut(&collateral)
                    .unwrap()
                    .treasury
                    .take_advanced(amount, WithdrawStrategy::Rounded(RoundingMode::ToZero));
            } else {
                return self
                    .collaterals
                    .get_mut(&collateral)
                    .unwrap()
                    .treasury
                    .take_advanced(amount, WithdrawStrategy::Rounded(RoundingMode::ToZero));
            }
        }

        /// Mint a controller badge
        pub fn mint_controller_badge(&self, amount: Decimal) -> Bucket {
            self.controller_badge_manager.mint(amount)
        }

        /// Edit a collateral's parameters
        pub fn edit_collateral(
            &mut self,
            address: ResourceAddress,
            new_mcr: Decimal,
            new_acceptance: bool,
            new_max_share: Decimal,
        ) {
            self.collaterals.get_mut(&address).unwrap().accepted = new_acceptance;
            self.collaterals.get_mut(&address).unwrap().mcr = new_mcr;
            self.collaterals.get_mut(&address).unwrap().max_stab_share = new_max_share;
        }

        /// Edit a pool collateral's parameters
        pub fn edit_pool_collateral(
            &mut self,
            address: ResourceAddress,
            new_acceptance: bool,
            new_max_share: Decimal,
        ) {
            self.pool_units.get_mut(&address).unwrap().accepted = new_acceptance;
            self.pool_units.get_mut(&address).unwrap().max_pool_share = new_max_share;
        }

        /// Set delay until a loan can be liquidated after marking (in minutes)
        pub fn set_liquidation_delay(&mut self, new_delay: i64) {
            self.parameters.liquidation_delay = new_delay;
        }

        /// Set delay until a loan can be liquited without marker, after it could be liquidated with a marker (in minutes)
        pub fn set_unmarked_delay(&mut self, new_delay: i64) {
            self.parameters.unmarked_delay = new_delay;
        }

        /// Set availability of liquidations, openings, closings, force minting and force liquidations
        pub fn set_stops(
            &mut self,
            liquidations: bool,
            openings: bool,
            closings: bool,
            force_mint: bool,
            force_liquidate: bool,
        ) {
            self.parameters.stop_closings = closings;
            self.parameters.stop_liquidations = liquidations;
            self.parameters.stop_openings = openings;
            self.parameters.stop_force_liquidate = force_liquidate;
            self.parameters.stop_force_mint = force_mint;
        }

        /// Set the maximum vector length for the collateral ratios (to prevent state explosion, vectors are non-lazily loaded)
        pub fn set_max_vector_length(&mut self, new_max_length: u64) {
            self.parameters.max_vector_length = new_max_length;
        }

        /// Set the minimum mintable amount of STAB (to prevent unprofitable liquidations)
        pub fn set_minimum_mint(&mut self, new_minimum_mint: Decimal) {
            self.parameters.minimum_mint = new_minimum_mint;
        }

        /// Set fines for being liquidated (for liquidators and the protocol)
        ///   - a liquidator fine of 0.05 and protocol fine of 0.03 would mean a liquidation would result in 1 + 0.05 + 0.03 = 1.08 times the minted STAB's value collateral being taken from the borrower.
        pub fn set_fines(&mut self, liquidator_fine: Decimal, stabilis_fine: Decimal) {
            self.parameters.liquidation_liquidation_fine = liquidator_fine;
            self.parameters.stabilis_liquidation_fine = stabilis_fine;
        }

        /// Set the force mint multiplier
        ///   - multiplier is used to calculate the minimum collateral ratio that will ever be reached through force minting
        ///       - a multiplier of 2, and an mcr of 1.5 would mean the lowest collateralization ratio reached by forced minting would be 300%
        pub fn set_force_mint_multiplier(&mut self, new_multiplier: Decimal) {
            self.parameters.force_mint_cr_multiplier = new_multiplier;
        }

        /// Gets the internal price of the STAB token
        pub fn return_internal_price(&self) -> Decimal {
            self.internal_stab_price
        }

        /// Mints free STAB (used by the flash loan component, for instance)
        pub fn free_stab(&mut self, amount: Decimal) -> Bucket {
            self.stab_manager.mint(amount)
        }

        /// Burns STAB
        pub fn burn_stab(&mut self, bucket: Bucket) {
            assert!(
                bucket.resource_address() == self.stab_manager.address(),
                "Can only burn STAB, not another token."
            );
            bucket.burn();
        }

        /// Burns a used marker
        pub fn burn_marker(&self, marker: Bucket) {
            let data: CdpMarker = marker.as_non_fungible().non_fungible().data();
            assert!(
                self.cdp_marker_manager.address() == marker.resource_address(),
                "Can only burn markers, not another token."
            );
            assert!(data.used, "Only used markers can be burned!");
            marker.burn();
        }

        /// Burns a used loan receipt (has to be liquidated, closed or force liquidated, and have no collateral left)
        pub fn burn_loan_receipt(&self, receipt: Bucket) {
            let data: Cdp = receipt.as_non_fungible().non_fungible().data();
            assert!(
                self.cdp_manager.address() == receipt.resource_address(),
                "Can only burn loan receipts, not another token."
            );
            assert!(
                data.status == CdpStatus::Liquidated
                    || data.status == CdpStatus::ForceLiquidated
                    || data.status == CdpStatus::Closed,
                "Loan not closed or liquidated"
            );
            assert!(
                data.collateral_amount == dec!(0),
                "Retrieve all collateral before burning!"
            );
            receipt.burn();
        }

        //HELPER METHODS

        /// Try to liquidate a CDP / loan
        ///
        /// # Input
        /// - `payment`: The STAB tokens to pay back
        /// - `cdp_data`: The CDP data
        /// - `marker_data`: The marker data
        /// - `marker_id`: The marker receipt id
        /// - `delay`: The delay until the loan can be liquidated from when it was marked
        ///
        /// # Output, depends on outcome:
        /// 1: liquidation successful
        /// - The collateral reward
        /// - The leftover STAB
        /// - A liquidation receipt
        /// 2: liquidation unsuccessful (because the loan was saved):
        /// - The STAB tokens
        /// - None
        /// - A liquidation receipt (with saved status)
        ///
        /// # Logic
        /// - Get the liquidation collateral ratio
        /// - Assert that liquidation is currently enabled, the marker is valid, the payment is sufficient, the time has passed, and the loan is marked
        /// - Get the newest collateral ratio for the CDP
        /// - Check whether the collateral ratio is sufficient, liquidate if not, save if it is
        fn try_liquidate(
            &mut self,
            payment: Bucket,
            cdp_data: Cdp,
            marker_data: CdpMarker,
            marker_id: NonFungibleLocalId,
            delay: i64,
        ) -> (Option<Bucket>, Option<Bucket>, Bucket) {
            let liquidation_collateral_ratio = self
                .collaterals
                .get(&cdp_data.parent_address)
                .unwrap()
                .liquidation_collateral_ratio;

            assert!(
                !self.parameters.stop_liquidations,
                "Not allowed to liquidate loans right now."
            );
            assert!(
                !marker_data.used && marker_data.mark_type == CdpUpdate::Marked,
                "Non-valid marker."
            );
            assert!(
                payment.amount() >= cdp_data.minted_stab,
                "not enough STAB supplied to close completely"
            );

            assert!(
                Clock::current_time_is_at_or_after(
                    marker_data.time_marked.add_minutes(delay).unwrap(),
                    TimePrecision::Second
                ),
                "Not yet able to liquidate, time now: {}, time marked: {}.",
                Clock::current_time_rounded_to_seconds().seconds_since_unix_epoch,
                marker_data.time_marked.seconds_since_unix_epoch
            );

            assert!(cdp_data.status == CdpStatus::Marked, "Loan not marked");

            let cr: Decimal = self.pool_to_real(
                cdp_data.collateral_amount,
                cdp_data.collateral,
                cdp_data.is_pool_unit_collateral,
            ) / cdp_data.minted_stab;

            if cr < liquidation_collateral_ratio {
                let (liquidation_payment, remainder, receipt): (Bucket, Bucket, Bucket) =
                    self.liquidate(payment, marker_data, marker_id, cdp_data, cr);
                (Some(liquidation_payment), Some(remainder), receipt)
            } else {
                let marker_receipt: Bucket = self.save(marker_data, cdp_data, cr);
                (None, Some(payment), marker_receipt)
            }
        }

        /// Liquidate a loan / CDP
        ///
        /// # Input
        /// - `payment`: The STAB tokens to pay back
        /// - `marker_data`: The marker data
        /// - `marker_id`: The marker receipt id
        /// - `cdp_data`: The CDP data
        /// - `cr`: The collateral ratio
        ///
        /// # Output
        /// - The collateral reward
        /// - The leftover STAB
        /// - A liquidation receipt
        ///
        /// # Logic
        /// - Update minted STAB
        /// - Update collateral amount of the parent address
        /// - Calculate the minimum collateral ratio and the liquidation collateral ratio
        /// - Create the liquidation receipt data
        /// - Update the marker and CDP receipts
        /// - Take the payment, check whether it's enough, and burn it
        /// - Calculate the liquidations according to the cr
        ///    - for calculation details, see code
        /// - Make the liquidation receipt
        /// - Handle calculated liquidations
        /// - Update liquidated cdp
        /// - Return the collateral reward, the leftover STAB and the liquidation receipt
        fn liquidate(
            &mut self,
            mut payment: Bucket,
            marker_data: CdpMarker,
            marker_id: NonFungibleLocalId,
            cdp_data: Cdp,
            cr: Decimal,
        ) -> (Bucket, Bucket, Bucket) {
            self.update_minted_stab(
                false,
                cdp_data.is_pool_unit_collateral,
                false,
                cdp_data.minted_stab,
                cdp_data.parent_address,
                cdp_data.collateral,
            );

            self.collaterals
                .get_mut(&cdp_data.parent_address)
                .unwrap()
                .collateral_amount -= cdp_data.collateral_stab_ratio * cdp_data.minted_stab;

            let mcr: Decimal = self.collaterals.get(&cdp_data.parent_address).unwrap().mcr;
            let liq_cr: Decimal = self
                .collaterals
                .get(&cdp_data.parent_address)
                .unwrap()
                .liquidation_collateral_ratio;
            let mut treasury_payment_amount: Option<Decimal> = None;
            let liquidation_payment_amount;
            let mut liquidation_receipt = LiquidationReceipt {
                collateral: cdp_data.collateral,
                stab_paid: cdp_data.minted_stab,
                percentage_owed: dec!(1) + self.parameters.liquidation_liquidation_fine,
                percentage_received: dec!(1) + self.parameters.liquidation_liquidation_fine,
                cdp_liquidated: marker_data.marked_id.clone(),
                date_liquidated: Clock::current_time_rounded_to_seconds(),
            };

            self.liquidation_counter += 1;

            self.marked_cdps.remove(&marker_data.marker_placing);
            self.marked_cdps_active -= 1;
            self.cdp_marker_manager
                .update_non_fungible_data(&marker_id, "used", true);
            self.cdp_manager.update_non_fungible_data(
                &marker_data.marked_id,
                "status",
                CdpStatus::Liquidated,
            );

            assert!(
                cdp_data.minted_stab <= payment.amount(),
                "Not enough STAB to liquidate."
            );
            let repayment: Bucket = payment.take(cdp_data.minted_stab);
            repayment.burn();

            //calculate the cr percentage, just the cr in percentage of the minted stab value
            //example: collateral value is $100, minted stab value is $80 -> cr = 100/80 = 1.25
            let cr_percentage: Decimal = mcr * cr / liq_cr;

            //calculate liquidations depending on cr
            //sit 1: cr > 1 + liquidation fine + stabilis fine   -> everyone can receive complete fines
            //sit 2: cr > 1 + liquidation fine                   -> liquidator receives whole fine, stabilis a partial fine
            //sit 3: cr <= 1                                     -> liquidator receives whole collateral, which might be less than minted stab

            if cr_percentage
                > dec!(1)
                    + self.parameters.liquidation_liquidation_fine
                    + self.parameters.stabilis_liquidation_fine
            {
                if self.parameters.stabilis_liquidation_fine > dec!(0) {
                    treasury_payment_amount = Some(
                        (self.parameters.stabilis_liquidation_fine)
                            * (cdp_data.collateral_amount / cr_percentage),
                    );
                }
                liquidation_payment_amount = (dec!(1)
                    + self.parameters.liquidation_liquidation_fine)
                    * (cdp_data.collateral_amount / cr_percentage);
            } else if cr_percentage > dec!(1) + self.parameters.liquidation_liquidation_fine {
                liquidation_payment_amount = (dec!(1)
                    + self.parameters.liquidation_liquidation_fine)
                    * (cdp_data.collateral_amount / cr_percentage);

                treasury_payment_amount =
                    Some(cdp_data.collateral_amount - liquidation_payment_amount);
            } else {
                liquidation_receipt.percentage_received = cr_percentage;
                liquidation_payment_amount = cdp_data.collateral_amount;
            }

            let receipt: NonFungibleBucket = self
                .liquidation_receipt_manager
                .mint_non_fungible(
                    &NonFungibleLocalId::integer(self.liquidation_counter),
                    liquidation_receipt,
                )
                .as_non_fungible();

            let treasury_payment = if let Some(payment_amount) = treasury_payment_amount {
                Some(self.take_collateral(
                    cdp_data.collateral,
                    cdp_data.is_pool_unit_collateral,
                    payment_amount,
                ))
            } else {
                None
            };

            let liquidation_payment = self.take_collateral(
                cdp_data.collateral,
                cdp_data.is_pool_unit_collateral,
                liquidation_payment_amount,
            );

            let leftover_collateral: Decimal = cdp_data.collateral_amount
                - liquidation_payment.amount()
                - treasury_payment
                    .as_ref()
                    .map_or(dec!(0), |payment_bucket| payment_bucket.amount());

            self.cdp_manager.update_non_fungible_data(
                &marker_data.marked_id,
                "collateral_amount",
                leftover_collateral,
            );

            if let Some(payment) = treasury_payment {
                self.put_collateral_in_treasury(
                    cdp_data.collateral,
                    cdp_data.is_pool_unit_collateral,
                    payment,
                );
            }

            (liquidation_payment, payment, receipt.into())
        }

        /// Save a loan / CDP
        ///
        /// # Input
        /// - `marker_data`: The marker data
        /// - `cdp_data`: The CDP data
        /// - `cr`: The collateral ratio
        ///
        /// # Output
        /// - A bucket with the liquidation receipt (a receipt specifying the loan was saved)
        ///
        /// # Logic
        /// - Update the collateral amount of the parent address
        /// - Create a marker for the savior
        /// - Create a liquidation receipt with saved status
        /// - Update the CDP to a healthy state and collateral ratio
        /// - Update the marker receipt to used
        /// - Insert the healthy CDP again
        /// - Return the liquidation receipt
        fn save(&mut self, marker_data: CdpMarker, cdp_data: Cdp, cr: Decimal) -> Bucket {
            self.collaterals
                .get_mut(&cdp_data.parent_address)
                .unwrap()
                .collateral_amount += (cr - cdp_data.collateral_stab_ratio) * cdp_data.minted_stab;

            self.marker_placing_counter += dec!(1);
            self.cdp_marker_counter += 1;

            let marker = CdpMarker {
                mark_type: CdpUpdate::Saved,
                time_marked: Clock::current_time_rounded_to_seconds(),
                marked_id: marker_data.marked_id.clone(),
                marker_placing: self.marker_placing_counter,
                used: false,
            };

            let marker_receipt: NonFungibleBucket = self
                .cdp_marker_manager
                .mint_non_fungible(
                    &NonFungibleLocalId::integer(self.cdp_marker_counter),
                    marker,
                )
                .as_non_fungible();

            self.marked_cdps.remove(&marker_data.marker_placing);
            self.marked_cdps_active -= 1;
            self.cdp_manager.update_non_fungible_data(
                &marker_data.marked_id,
                "status",
                CdpStatus::Healthy,
            );
            self.cdp_manager.update_non_fungible_data(
                &marker_data.marked_id,
                "collateral_stab_ratio",
                cr,
            );
            self.cdp_marker_manager.update_non_fungible_data(
                &NonFungibleLocalId::integer(cdp_data.marker_id),
                "used",
                true,
            );

            self.insert_cr(cdp_data.parent_address, cr, marker_data.marked_id.clone());

            marker_receipt.into()
        }

        /// Insert a collateral ratio into the AvlTree
        fn insert_cr(
            &mut self,
            parent_address: ResourceAddress,
            cr: Decimal,
            cdp_id: NonFungibleLocalId,
        ) {
            if self
                .collateral_ratios
                .get_mut(&parent_address)
                .unwrap()
                .get_mut(&cr)
                .is_some()
            {
                let mut cdp_ids: Vec<NonFungibleLocalId> = self
                    .collateral_ratios
                    .get_mut(&parent_address)
                    .unwrap()
                    .get_mut(&cr)
                    .unwrap()
                    .clone()
                    .to_vec();
                assert!(
                    cdp_ids.len() < self.parameters.max_vector_length.try_into().unwrap(),
                    "CR vector is full..."
                );
                cdp_ids.push(cdp_id);
                self.collateral_ratios
                    .get_mut(&parent_address)
                    .unwrap()
                    .insert(cr, cdp_ids);
            } else {
                let cdp_ids: Vec<NonFungibleLocalId> = vec![cdp_id];
                self.collateral_ratios
                    .get_mut(&parent_address)
                    .unwrap()
                    .insert(cr, cdp_ids);
            }

            if self.collaterals.get(&parent_address).unwrap().highest_cr < cr {
                self.collaterals
                    .get_mut(&parent_address)
                    .unwrap()
                    .highest_cr = cr;
            }
        }

        /// Remove a collateral ratio from the AvlTree
        fn remove_cr(
            &mut self,
            parent_address: ResourceAddress,
            cr: Decimal,
            receipt_id: NonFungibleLocalId,
        ) {
            let mut collateral_ids: Vec<NonFungibleLocalId> = self
                .collateral_ratios
                .get_mut(&parent_address)
                .unwrap()
                .get_mut(&cr)
                .unwrap()
                .to_vec();

            collateral_ids.retain(|id| id != &receipt_id);

            self.collateral_ratios
                .get_mut(&parent_address)
                .unwrap()
                .insert(cr, collateral_ids.clone());

            if collateral_ids.is_empty() {
                self.collateral_ratios
                    .get_mut(&parent_address)
                    .unwrap()
                    .remove(&cr);
            }
        }

        /// Calculate the real value of a pool collateral, if it is a pool unit
        ///    - Example: a resource is an LSU, 1 LSU = 1.1 XRD. If the collateral amount is 10 LSU, 11 XRD is returned.
        fn pool_to_real(
            &mut self,
            amount: Decimal,
            collateral: ResourceAddress,
            pool: bool,
        ) -> Decimal {
            if pool {
                if self.pool_units.get_mut(&collateral).unwrap().lsu {
                    self.pool_units
                        .get_mut(&collateral)
                        .unwrap()
                        .validator
                        .unwrap()
                        .get_redemption_value(amount)
                } else {
                    self.pool_units
                        .get_mut(&collateral)
                        .unwrap()
                        .one_resource_pool
                        .unwrap()
                        .get_redemption_value(amount)
                }
            } else {
                amount
            }
        }

        /// Check whether a collateral's share is too big
        fn check_share(
            &mut self,
            parent_collateral_address: ResourceAddress,
            is_pool_unit_collateral: bool,
            collateral_address: ResourceAddress,
        ) {
            assert!(
                self.collaterals
                    .get(&parent_collateral_address)
                    .unwrap()
                    .minted_stab
                    / self.circulating_stab
                    <= self
                        .collaterals
                        .get(&parent_collateral_address)
                        .unwrap()
                        .max_stab_share,
                "This collateral's share is too big already"
            );
            if is_pool_unit_collateral {
                assert!(
                    self.pool_units
                        .get(&collateral_address)
                        .unwrap()
                        .minted_stab
                        / self
                            .collaterals
                            .get(&parent_collateral_address)
                            .unwrap()
                            .minted_stab
                        <= self
                            .pool_units
                            .get(&collateral_address)
                            .unwrap()
                            .max_pool_share,
                    "This pool collateral's share is too big already"
                );
            }
        }

        /// Update minted STAB
        fn update_minted_stab(
            &mut self,
            add: bool,
            is_pool_unit_collateral: bool,
            check_share: bool,
            amount: Decimal,
            collateral: ResourceAddress,
            pool_unit: ResourceAddress,
        ) {
            if add {
                self.collaterals.get_mut(&collateral).unwrap().minted_stab += amount;
                if is_pool_unit_collateral {
                    self.pool_units.get_mut(&pool_unit).unwrap().minted_stab += amount;
                }
                self.circulating_stab += amount;
            } else {
                self.collaterals.get_mut(&collateral).unwrap().minted_stab -= amount;
                if is_pool_unit_collateral {
                    self.pool_units.get_mut(&pool_unit).unwrap().minted_stab -= amount;
                }
                self.circulating_stab -= amount;
            }

            if check_share {
                self.check_share(collateral, is_pool_unit_collateral, pool_unit);
            }
        }

        /// Take collateral out of the correct vault
        fn take_collateral(
            &mut self,
            collateral: ResourceAddress,
            pool: bool,
            amount: Decimal,
        ) -> Bucket {
            if pool {
                self.pool_units
                    .get_mut(&collateral)
                    .unwrap()
                    .vault
                    .take_advanced(amount, WithdrawStrategy::Rounded(RoundingMode::ToZero))
            } else {
                self.collaterals
                    .get_mut(&collateral)
                    .unwrap()
                    .vault
                    .take_advanced(amount, WithdrawStrategy::Rounded(RoundingMode::ToZero))
            }
        }

        /// Put collateral in the correct vault
        fn put_collateral(
            &mut self,
            collateral: ResourceAddress,
            pool: bool,
            collateral_bucket: Bucket,
        ) {
            if pool {
                self.pool_units
                    .get_mut(&collateral)
                    .unwrap()
                    .vault
                    .put(collateral_bucket)
            } else {
                self.collaterals
                    .get_mut(&collateral)
                    .unwrap()
                    .vault
                    .put(collateral_bucket)
            }
        }

        /// Put collateral in the treasury
        fn put_collateral_in_treasury(
            &mut self,
            collateral: ResourceAddress,
            pool: bool,
            collateral_bucket: Bucket,
        ) {
            if pool {
                self.pool_units
                    .get_mut(&collateral)
                    .unwrap()
                    .treasury
                    .put(collateral_bucket)
            } else {
                self.collaterals
                    .get_mut(&collateral)
                    .unwrap()
                    .treasury
                    .put(collateral_bucket)
            }
        }
    }
}

#[derive(ScryptoSbor)]
/// All info about a collateral used by the protocol
pub struct CollateralInfo {
    pub mcr: Decimal,
    pub usd_price: Decimal,
    pub liquidation_collateral_ratio: Decimal,
    pub vault: Vault,
    pub resource_address: ResourceAddress,
    pub treasury: Vault,
    pub accepted: bool,
    pub initialized: bool,
    pub max_stab_share: Decimal,
    pub minted_stab: Decimal,
    pub collateral_amount: Decimal,
    pub highest_cr: Decimal,
}

#[derive(ScryptoSbor)]
pub struct PoolUnitInfo {
    pub vault: Vault,
    pub treasury: Vault,
    pub lsu: bool,
    pub validator: Option<Global<Validator>>,
    pub one_resource_pool: Option<Global<OneResourcePool>>,
    pub parent_address: ResourceAddress,
    pub address: ResourceAddress,
    pub accepted: bool,
    pub minted_stab: Decimal,
    pub max_pool_share: Decimal,
}

#[derive(ScryptoSbor)]
pub struct ProtocolParameters {
    pub minimum_mint: Decimal,
    pub max_vector_length: u64,
    pub liquidation_delay: i64,
    pub unmarked_delay: i64,
    pub liquidation_liquidation_fine: Decimal,
    pub stabilis_liquidation_fine: Decimal,
    pub stop_liquidations: bool,
    pub stop_openings: bool,
    pub stop_closings: bool,
    pub stop_force_mint: bool,
    pub stop_force_liquidate: bool,
    pub force_mint_cr_multiplier: Decimal,
}
