//! # STAB module proxy component blueprint
//!
//! The proxy component is used to interact with the Stabilis component. It is used to:
//! - Update the Stabilis component with new parameters / data, such as:
//!    - STAB's internal price
//!        - The internal price is calculated by the interest rate within this component
//!             - The interest rate is calculated using a PID controller (with the price error as the input), to ensure demand and supply for STAB meets at the price: STAB trading above its peg will decrease the interest rate (to incentivize borrowing), and vice versa.
//!    - Collateral prices
//! - Ensure that the Stabilis component is only interacted with by authorized callers.
//! - Ensure potential upgrades to the Stabilis component can be done without disrupting the rest of the system.
//!
//! Interest rate calculation is done within the proxy component, and collateral prices are gathered from an oracle and sent to the main component through here as well.
//!
//! Methods used to call other components only are explained in their respective modules.
//! Sometimes, a proof is checked within this component, as they cannot be passed along to other components. The ID for this proof is then passed along, for the other component to check the proofs data.

use crate::flash_loans::flash_loans::*;
use crate::oracle::oracle::*;
use crate::shared_structs::*;
use crate::stabilis_component::stabilis_component::*;
use crate::stabilis_liquidity_pool::stabilis_liquidity_pool::*;
use scrypto::prelude::*;
use scrypto_math::*;

#[blueprint]
#[types(
    ResourceAddress,
    bool,
    Decimal,
    CdpStatus,
    u64,
    CdpUpdate,
    Instant,
    NonFungibleLocalId
)]
mod proxy {
    enable_method_auth! {
        methods {
            open_cdp => PUBLIC;
            borrow_more => PUBLIC;
            top_up_cdp => PUBLIC;
            remove_collateral => PUBLIC;
            close_cdp => PUBLIC;
            partial_close_cdp => PUBLIC;
            retrieve_leftover_collateral => PUBLIC;
            mark_for_liquidation => PUBLIC;
            liquidate_position_with_marker => PUBLIC;
            liquidate_position_without_marker => PUBLIC;
            update => PUBLIC;
            get_internal_price => PUBLIC;
            flash_borrow => PUBLIC;
            flash_pay_back => PUBLIC;
            burn_marker => PUBLIC;
            burn_loan_receipt => PUBLIC;
            force_mint => PUBLIC;
            force_liquidate => PUBLIC;
            receive_badges => PUBLIC;
            change_collateral_price => restrict_to: [OWNER];
            set_max_vector_length => restrict_to: [OWNER];
            set_price_error => restrict_to: [OWNER];
            set_minmax_interest => restrict_to: [OWNER];
            set_update_delays => restrict_to: [OWNER];
            set_ks => restrict_to: [OWNER];
            set_allowed_deviation => restrict_to: [OWNER];
            add_collateral => restrict_to: [OWNER];
            add_pool_collateral => restrict_to: [OWNER];
            change_internal_price => restrict_to: [OWNER];
            set_oracle => restrict_to: [OWNER];
            send_badges => restrict_to: [OWNER];
            flash_retrieve_interest => restrict_to: [OWNER];
            set_force_mint_liq_percentage => restrict_to: [OWNER];
            set_number_of_prices_cached => restrict_to: [OWNER];
            add_pair_to_oracle => restrict_to: [OWNER];
            set_reward_per_second => restrict_to: [OWNER];
            put_reward_in_vault => PUBLIC;
            add_claimed_website => restrict_to: [OWNER];
        }
    }

    struct Proxy {
        /// The vault for the controller badge, used to authorize method calls to the Stabilis component
        badge_vault: FungibleVault,
        /// The global instance of the StabilisPool component
        stab_pool: Global<StabilisPool>,
        /// The global instance of the Stabilis component
        stabilis: Global<Stabilis>,
        /// The global instance of the oracle component
        oracle: Global<AnyComponent>,
        /// The name of the method to call on the oracle component
        oracle_method_name: String,
        /// The global instance of the flash loans component
        flash_loans: Global<FlashLoans>,
        /// The delay between updates (minutes)
        update_delay: i64,
        /// The number of cached prices to use for the interest rate calculation
        number_of_cached_prices: u64,
        /// The resource manager for the CDP receipts
        cdp_receipt_manager: ResourceManager,
        /// The resource manager for the CDP markers
        cdp_marker_manager: ResourceManager,
        /// The price of the XRD token
        xrd_price: Decimal,
        /// The collaterals accepted by the Stabilis component
        accepted_collaterals: HashMap<ResourceAddress, u64>,
        /// The percentage of the collateral to supply when force minting
        percentage_to_supply: Decimal,
        /// The percentage of the collateral to take when force liquidating
        percentage_to_take: Decimal,
        /// The Interest Rate parameters
        parameters: InterestParameters,
        /// Data about STAB's price
        stab_price_data: StabPriceData,
        /// Reward vault for updating the prices
        reward_vault: Vault,
        /// The reward per second for updating the prices
        reward_per_second: Decimal,
        /// The dapp definition account
        dapp_def_account: Global<Account>,
    }

    impl Proxy {
        /// Instantiates the Proxy component, a StabilisPool component and a FlashLoans component for the Stabilis protocol
        ///
        /// # Input
        /// - `xrd_bucket`: The bucket for the XRD token
        /// - `stab_bucket`: The bucket for the STAB token
        /// - `controller_badge`: The controller badge of the Stabilis component
        /// - `cdp_receipt_address`: The resource address of the CDP receipts (created by the Stabilis component)
        /// - `cdp_marker_address`: The resource address of the CDP markers (created by the Stabilis component)
        /// - `oracle_address`: The address of the oracle component
        /// - `stabilis_address`: The address of the Stabilis component
        ///
        /// # Output
        /// - The global instance of the Proxy component
        /// - The bucket for the LP tokens (STAB/XRD, generated by the StabilisPool component)
        /// - The optional bucket for the leftover LP tokens (STAB/XRD, generated by the StabilisPool component)
        ///
        /// # Logic
        /// - Instantiates the StabilisPool component
        ///     - Adds liquidity to the STAB/XRD pool
        /// - Gets the internal price of the STAB token
        /// - Instantiates the FlashLoans component
        /// - Instantiates the Proxy component
        pub fn new(
            xrd_bucket: Bucket,
            stab_bucket: Bucket,
            mut controller_badge: Bucket,
            owner_role: OwnerRole,
            morpher_oracle_address: ComponentAddress,
            cdp_receipt_address: ResourceAddress,
            cdp_marker_address: ResourceAddress,
            stabilis_address: ComponentAddress,
            reward_address: ResourceAddress,
        ) -> (Global<Proxy>, Bucket, Option<Bucket>) {
            let (address_reservation, component_address) =
                Runtime::allocate_component_address(Proxy::blueprint_id());

            let dapp_def_account =
                Blueprint::<Account>::create_advanced(OwnerRole::Updatable(rule!(allow_all)), None); // will reset owner role after dapp def metadata has been set
            let dapp_def_address = GlobalAddress::from(dapp_def_account.address());

            let stabilis: Global<Stabilis> = Global::from(stabilis_address);

            let controller_address: ResourceAddress = controller_badge.resource_address();

            let stab_pool: Global<StabilisPool> = StabilisPool::new(
                OwnerRole::Fixed(rule!(require(controller_address))),
                stab_bucket.resource_address(),
                xrd_bucket.resource_address(),
                dec!(0.001),
                dapp_def_address,
            );

            let (lp_tokens, optional_return_bucket): (Bucket, Option<Bucket>) =
                stab_pool.add_liquidity(stab_bucket, xrd_bucket);

            let internal_price: Decimal =
                controller_badge.authorize_with_all(|| stabilis.return_internal_price());

            let mut accepted_collaterals: HashMap<ResourceAddress, u64> = HashMap::new();
            accepted_collaterals.insert(
                XRD,
                Clock::current_time_rounded_to_seconds().seconds_since_unix_epoch as u64,
            );

            let own_oracle_address: ComponentAddress = Oracle::instantiate_oracle(
                owner_role.clone(),
                morpher_oracle_address,
                dapp_def_address,
            )
            .address();

            let flash_loans = FlashLoans::instantiate(
                controller_badge.take(1),
                Global::from(stabilis_address),
                dapp_def_address,
            );

            dapp_def_account.set_metadata("account_type", String::from("dapp definition"));
            dapp_def_account.set_metadata("name", "STAB Protocol".to_string());
            dapp_def_account
                .set_metadata("description", "Bringing stable assets to Radix".to_string());
            dapp_def_account.set_metadata("info_url", Url::of("https://ilikeitstable.com"));
            dapp_def_account.set_metadata(
                "icon_url",
                Url::of("https://ilikeitstable.com/images/stablogo.png"),
            );
            dapp_def_account.set_metadata(
                "claimed_websites",
                vec![
                    Url::of("https://ilikeitstable.com"),
                    Url::of("https://beta.ilikeitstable.com"),
                ],
            );
            dapp_def_account.set_metadata(
                "claimed_entities",
                vec![
                    GlobalAddress::from(component_address.clone()),
                    GlobalAddress::from(stabilis_address),
                    GlobalAddress::from(flash_loans.address()),
                    GlobalAddress::from(own_oracle_address),
                    GlobalAddress::from(stab_pool.address()),
                ],
            );
            dapp_def_account.set_owner_role(rule!(require(controller_badge.resource_address())));

            let proxy = Self {
                flash_loans,
                badge_vault: FungibleVault::with_bucket(controller_badge.as_fungible()),
                stab_pool,
                stabilis,
                oracle: Global::from(own_oracle_address),
                oracle_method_name: "get_prices".to_string(),
                update_delay: 1,
                number_of_cached_prices: 50,
                cdp_receipt_manager: ResourceManager::from_address(cdp_receipt_address),
                cdp_marker_manager: ResourceManager::from_address(cdp_marker_address),
                xrd_price: dec!("0.041"),
                accepted_collaterals,
                percentage_to_supply: dec!("1.05"),
                percentage_to_take: dec!("0.95"),
                stab_price_data: StabPriceData {
                    latest_stab_price_errors: ProxyKeyValueStore::new_with_registered_type(),
                    latest_stab_price_errors_total: dec!(0),
                    last_update: Clock::current_time_rounded_to_seconds(),
                    last_changed_price: 0,
                    internal_price,
                    full_cache: false,
                    interest_rate: dec!(1),
                },
                parameters: InterestParameters {
                    kp: dec!("0.00000000076517857"),
                    ki: dec!("0.00000000076517857"),
                    max_interest_rate: dec!("1.0000007715"),
                    min_interest_rate: dec!("0.9999992287"),
                    allowed_deviation: dec!("0.005"),
                    price_error_offset: dec!(1),
                    max_price_error: dec!(0.5),
                },
                reward_vault: Vault::new(reward_address),
                reward_per_second: dec!("0.02"),
                dapp_def_account,
            }
            .instantiate()
            .prepare_to_globalize(owner_role)
            .with_address(address_reservation)
            .metadata(metadata! {
                init {
                    "name" => "STAB Protocol Proxy".to_string(), updatable;
                    "description" => "A proxy component for the STAB Protocol".to_string(), updatable;
                    "info_url" => Url::of("https://ilikeitstable.com"), updatable;
                    "dapp_definition" => dapp_def_address, updatable;
                }
            })
            .globalize();

            (proxy, lp_tokens, optional_return_bucket)
        }

        /// Updates the Stabilis component with new data
        ///
        /// # Input
        /// - None
        ///
        /// # Output
        /// - None
        ///
        /// # Logic
        /// - Updates the collateral prices
        /// - Checks if the internal price needs to be updated
        /// - Updates the internal price if needed
        pub fn update(&mut self) -> Option<Bucket> {
            self.update_internal_price();
            self.update_collateral_prices()
        }

        /// Receives controller badges
        pub fn receive_badges(&mut self, badge_bucket: Bucket) {
            self.badge_vault.put(badge_bucket.as_fungible());
        }

        //==================================================================
        //                         ADMIN METHODS
        //==================================================================

        /// Sets the price error parameters
        pub fn set_price_error(&mut self, new_max: Decimal, new_offset: Decimal) {
            self.parameters.allowed_deviation = new_max;
            self.parameters.price_error_offset = new_offset;
        }

        /// Sets the allowed deviation for the internal price
        pub fn set_allowed_deviation(&mut self, allowed_deviation: Decimal) {
            self.parameters.allowed_deviation = allowed_deviation;
        }

        /// Sets the number of prices to cache for the interest rate calculation
        pub fn set_number_of_prices_cached(&mut self, new_number: u64) {
            self.number_of_cached_prices = new_number;
            self.stab_price_data.latest_stab_price_errors_total = dec!(0);
            self.stab_price_data.last_changed_price = 0;
            self.stab_price_data.full_cache = false;
        }

        /// Sets the percentage to supply and take when force minting and liquidating
        pub fn set_force_mint_liq_percentage(
            &mut self,
            percentage_to_supply: Decimal,
            percentage_to_take: Decimal,
        ) {
            self.percentage_to_supply = percentage_to_supply;
            self.percentage_to_take = percentage_to_take;
        }

        /// Sets the min/max interest rate parameters
        pub fn set_minmax_interest(&mut self, min_interest: Decimal, max_interest: Decimal) {
            self.parameters.max_interest_rate = max_interest;
            self.parameters.min_interest_rate = min_interest;
        }

        /// Sets the update delays
        pub fn set_update_delays(&mut self, update_delay: i64) {
            self.update_delay = update_delay;
        }

        /// Sets the k values for the interest rate calculation
        pub fn set_ks(&mut self, new_ki: Decimal, new_kp: Decimal) {
            self.parameters.ki = new_ki;
            self.parameters.kp = new_kp;
        }

        /// Sets the oracle component and method to call
        pub fn set_oracle(&mut self, oracle_address: ComponentAddress, method_name: String) {
            self.oracle = Global::from(oracle_address);
            self.oracle_method_name = method_name;
        }

        /// Sends badges to another component
        pub fn send_badges(&mut self, amount: Decimal, receiver_address: ComponentAddress) {
            let receiver: Global<AnyComponent> = Global::from(receiver_address);
            let badge_bucket: Bucket = self.badge_vault.take(amount).into();
            receiver.call_raw("receive_badges", scrypto_args!(badge_bucket))
        }

        /// Sets the reward per second for updating the prices
        pub fn set_reward_per_second(&mut self, reward_per_second: Decimal) {
            self.reward_per_second = reward_per_second;
        }

        /// Puts the reward in the reward vault
        pub fn put_reward_in_vault(&mut self, rewards: Bucket) {
            self.reward_vault.put(rewards);
        }

        /// Adds claimed website to the dapp definition
        pub fn add_claimed_website(&mut self, website: Url) {
            match self.dapp_def_account.get_metadata("claimed_websites") {
                Ok(Some(claimed_websites)) => {
                    let mut claimed_websites: Vec<Url> = claimed_websites;
                    claimed_websites.push(website);
                    self.badge_vault.authorize_with_amount(dec!("0.75"), || {
                        self.dapp_def_account
                            .set_metadata("claimed_websites", claimed_websites);
                    });
                }
                Ok(None) | Err(_) => {
                    self.badge_vault.authorize_with_amount(dec!("0.75"), || {
                        self.dapp_def_account
                            .set_metadata("claimed_websites", vec![website]);
                    });
                }
            }
        }

        //==================================================================
        //                         HELPER METHODS
        //==================================================================

        /// Updates the collateral prices of the Stabilis component
        ///
        /// # Input
        /// - None
        ///
        /// # Output
        /// - None
        ///
        /// # Logic
        /// - Calls the oracle component to get the latest prices
        /// - Iterates over them and updates the collateral prices in the Stabilis component
        fn update_collateral_prices(&mut self) -> Option<Bucket> {
            let prices: Vec<(ResourceAddress, Decimal, u64, String)> =
                self.oracle.call(&self.oracle_method_name, &());

            let mut updated_seconds: u64 = 0;

            for (address, price, timestamp, _pair) in prices {
                if let Some(stored_timestamp) = self.accepted_collaterals.get_mut(&address) {
                    if address == XRD {
                        self.xrd_price = price;
                    }
                    self.badge_vault.authorize_with_amount(dec!("0.75"), || {
                        self.stabilis.change_collateral_price(address, price)
                    });
                    updated_seconds += timestamp - stored_timestamp.clone();
                    *stored_timestamp = timestamp;
                }
            }
            let reward: Decimal = Decimal::from(updated_seconds) * self.reward_per_second;
            if self.reward_vault.amount() > reward {
                Some(self.reward_vault.take(reward))
            } else {
                None
            }
        }

        /// Updates the internal price of the STAB token
        ///
        /// # Input
        /// - None
        ///
        /// # Output
        /// - None
        ///
        /// # Logic
        /// - Calculates the price error
        /// - Updates the latest price errors
        ///   - System keeps track of the latest n (by default 50) price errors and their totals
        ///     - If this cache is full, it replaces the oldest price error with the new one
        /// - Calculates the new interest rate using the PID controller
        /// - Updates the internal price using the new interest rate
        fn update_internal_price(&mut self) {
            let passed_minutes: Decimal = (Clock::current_time_rounded_to_seconds()
                .seconds_since_unix_epoch
                - self.stab_price_data.last_update.seconds_since_unix_epoch)
                / dec!(60);

            if passed_minutes < Decimal::from(self.update_delay) {
                return;
            }

            let mut price_error: Decimal = self.stab_pool.get_stab_price()
                * self.xrd_price
                * self.parameters.price_error_offset
                - self.stab_price_data.internal_price;

            if price_error > self.parameters.max_price_error {
                price_error = self.parameters.max_price_error;
            }

            let to_change_id: u64 =
                match self.stab_price_data.last_changed_price >= self.number_of_cached_prices {
                    true => {
                        self.stab_price_data.full_cache = true;
                        1
                    }
                    false => self.stab_price_data.last_changed_price + 1,
                };

            if !self.stab_price_data.full_cache {
                self.stab_price_data.latest_stab_price_errors_total += price_error;
            } else {
                self.stab_price_data.latest_stab_price_errors_total += price_error
                    - *self
                        .stab_price_data
                        .latest_stab_price_errors
                        .get(&to_change_id)
                        .unwrap();
            }

            self.stab_price_data.last_changed_price = to_change_id;
            self.stab_price_data
                .latest_stab_price_errors
                .insert(to_change_id, price_error);

            if price_error.checked_abs().unwrap()
                > self.parameters.allowed_deviation * self.stab_price_data.internal_price
            {
                self.stab_price_data.interest_rate -= (self.parameters.kp
                    * (price_error / self.stab_price_data.internal_price)
                    + self.parameters.ki
                        * (self.stab_price_data.latest_stab_price_errors_total
                            / (self.stab_price_data.internal_price
                                * Decimal::from(self.number_of_cached_prices))))
                    * passed_minutes;

                if self.stab_price_data.interest_rate > self.parameters.max_interest_rate {
                    self.stab_price_data.interest_rate = self.parameters.max_interest_rate;
                } else if self.stab_price_data.interest_rate < self.parameters.min_interest_rate {
                    self.stab_price_data.interest_rate = self.parameters.min_interest_rate;
                }
            }

            let calculated_price: Decimal = self.stab_price_data.internal_price
                * self
                    .stab_price_data
                    .interest_rate
                    .pow(passed_minutes)
                    .unwrap();

            self.stab_price_data.last_update = Clock::current_time_rounded_to_seconds();
            self.change_internal_price(calculated_price);
        }

        //==================================================================
        //    PROXY FUNCTIONALITY FROM HERE (CONTROL OTHER COMPONENTS)
        //==================================================================

        //==================================================================
        //                       STABILIS COMPONENT
        //==================================================================

        pub fn open_cdp(&mut self, collateral: Bucket, stab_to_mint: Decimal) -> (Bucket, Bucket) {
            self.badge_vault.authorize_with_amount(dec!("0.75"), || {
                self.stabilis.open_cdp(collateral, stab_to_mint)
            })
        }

        pub fn borrow_more(&mut self, receipt_proof: NonFungibleProof, amount: Decimal) -> Bucket {
            let receipt_proof = receipt_proof.check_with_message(
                self.cdp_receipt_manager.address(),
                "Incorrect proof! Are you sure this loan is yours?",
            );
            let receipt = receipt_proof.non_fungible::<Cdp>();
            let receipt_id: NonFungibleLocalId = receipt.local_id().clone();

            self.badge_vault.authorize_with_amount(dec!("0.75"), || {
                self.stabilis.borrow_more(receipt_id, amount)
            })
        }

        pub fn add_collateral(
            &mut self,
            address: ResourceAddress,
            chosen_mcr: Decimal,
            initial_price: Decimal,
        ) {
            self.badge_vault.authorize_with_amount(dec!("0.75"), || {
                self.stabilis
                    .add_collateral(address, chosen_mcr, initial_price)
            });
            self.accepted_collaterals.insert(
                address,
                Clock::current_time_rounded_to_seconds().seconds_since_unix_epoch as u64,
            );
        }

        pub fn remove_collateral(
            &mut self,
            receipt_proof: NonFungibleProof,
            amount: Decimal,
        ) -> Bucket {
            let receipt_proof = receipt_proof.check_with_message(
                self.cdp_receipt_manager.address(),
                "Incorrect proof! Are you sure this loan is yours?",
            );
            let receipt = receipt_proof.non_fungible::<Cdp>();
            let receipt_id: NonFungibleLocalId = receipt.local_id().clone();

            self.badge_vault.authorize_with_amount(dec!("0.75"), || {
                self.stabilis.remove_collateral(receipt_id, amount)
            })
        }

        pub fn close_cdp(
            &mut self,
            receipt_proof: NonFungibleProof,
            stab_payment: Bucket,
        ) -> (Bucket, Bucket) {
            let receipt_proof = receipt_proof.check_with_message(
                self.cdp_receipt_manager.address(),
                "Incorrect proof! Are you sure this loan is yours?",
            );
            let receipt = receipt_proof.non_fungible::<Cdp>();
            let receipt_id: NonFungibleLocalId = receipt.local_id().clone();

            self.badge_vault.authorize_with_amount(dec!("0.75"), || {
                self.stabilis.close_cdp(receipt_id, stab_payment)
            })
        }

        pub fn partial_close_cdp(
            &mut self,
            receipt_proof: NonFungibleProof,
            stab_payment: Bucket,
        ) -> (Option<Bucket>, Option<Bucket>) {
            let receipt_proof = receipt_proof.check_with_message(
                self.cdp_receipt_manager.address(),
                "Incorrect proof! Are you sure this loan is yours?",
            );
            let receipt = receipt_proof.non_fungible::<Cdp>();
            let receipt_id: NonFungibleLocalId = receipt.local_id().clone();

            self.badge_vault.authorize_with_amount(dec!("0.75"), || {
                self.stabilis.partial_close_cdp(receipt_id, stab_payment)
            })
        }

        pub fn retrieve_leftover_collateral(&mut self, receipt_proof: NonFungibleProof) -> Bucket {
            let receipt_proof = receipt_proof.check_with_message(
                self.cdp_receipt_manager.address(),
                "Incorrect proof! Are you sure this loan is yours?",
            );
            let receipt = receipt_proof.non_fungible::<Cdp>();
            let receipt_id: NonFungibleLocalId = receipt.local_id().clone();

            self.badge_vault.authorize_with_amount(dec!("0.75"), || {
                self.stabilis.retrieve_leftover_collateral(receipt_id)
            })
        }

        pub fn top_up_cdp(&mut self, receipt_proof: NonFungibleProof, collateral: Bucket) {
            let receipt_proof = receipt_proof.check_with_message(
                self.cdp_receipt_manager.address(),
                "Incorrect proof! Are you sure this loan is yours?",
            );
            let receipt = receipt_proof.non_fungible::<Cdp>();
            let receipt_id: NonFungibleLocalId = receipt.local_id().clone();

            self.badge_vault.authorize_with_amount(dec!("0.75"), || {
                self.stabilis.top_up_cdp(receipt_id, collateral)
            });
        }

        pub fn mark_for_liquidation(&mut self, collateral: ResourceAddress) -> Bucket {
            self.badge_vault.authorize_with_amount(dec!("0.75"), || {
                self.stabilis.mark_for_liquidation(collateral)
            })
        }

        pub fn burn_marker(&self, marker: Bucket) {
            self.badge_vault
                .authorize_with_amount(dec!("0.75"), || self.stabilis.burn_marker(marker));
        }

        pub fn burn_loan_receipt(&self, receipt: Bucket) {
            self.badge_vault
                .authorize_with_amount(dec!("0.75"), || self.stabilis.burn_loan_receipt(receipt));
        }

        pub fn liquidate_position_with_marker(
            &mut self,
            marker_proof: NonFungibleProof,
            payment: Bucket,
        ) -> (Option<Bucket>, Option<Bucket>, Bucket) {
            let marker_proof = marker_proof.check_with_message(
                self.cdp_marker_manager.address(),
                "Incorrect proof! Are you sure this is a correct marker?",
            );
            let marker = marker_proof.non_fungible::<CdpMarker>();
            let marker_id: NonFungibleLocalId = marker.local_id().clone();

            self.badge_vault.authorize_with_amount(dec!("0.75"), || {
                self.stabilis
                    .liquidate_position_with_marker(marker_id, payment)
            })
        }

        pub fn force_liquidate(
            &mut self,
            collateral: ResourceAddress,
            payment: Bucket,
        ) -> (Bucket, Bucket) {
            self.badge_vault.authorize_with_amount(dec!("0.75"), || {
                self.stabilis
                    .force_liquidate(collateral, payment, self.percentage_to_take, true)
            })
        }

        pub fn force_mint(
            &mut self,
            collateral: ResourceAddress,
            payment: Bucket,
        ) -> (Bucket, Option<Bucket>) {
            self.badge_vault.authorize_with_amount(dec!("0.75"), || {
                self.stabilis
                    .force_mint(collateral, payment, self.percentage_to_supply)
            })
        }

        pub fn liquidate_position_without_marker(
            &mut self,
            payment: Bucket,
            skip: Option<i64>,
            cdp_id: NonFungibleLocalId,
        ) -> (Option<Bucket>, Option<Bucket>, Bucket) {
            self.badge_vault.authorize_with_amount(dec!("0.75"), || {
                self.stabilis
                    .liquidate_position_without_marker(payment, skip, cdp_id)
            })
        }

        pub fn change_collateral_price(&self, collateral: ResourceAddress, new_price: Decimal) {
            self.badge_vault.authorize_with_amount(dec!("0.75"), || {
                self.stabilis.change_collateral_price(collateral, new_price)
            });
        }

        pub fn add_pool_collateral(
            &self,
            address: ResourceAddress,
            parent_address: ResourceAddress,
            validator: ComponentAddress,
            lsu: bool,
            initial_acceptance: bool,
        ) {
            self.badge_vault.authorize_with_amount(dec!("0.75"), || {
                self.stabilis.add_pool_collateral(
                    address,
                    parent_address,
                    validator,
                    lsu,
                    initial_acceptance,
                )
            });
        }

        pub fn change_internal_price(&mut self, new_price: Decimal) {
            self.stab_price_data.internal_price = new_price;
            self.badge_vault.authorize_with_amount(dec!("0.75"), || {
                self.stabilis.change_internal_price(new_price)
            });
        }

        pub fn set_max_vector_length(&mut self, new_stabilis_length: u64) {
            self.badge_vault.authorize_with_amount(dec!("0.75"), || {
                self.stabilis.set_max_vector_length(new_stabilis_length)
            });
        }

        pub fn get_internal_price(&self) -> Decimal {
            self.stab_price_data.internal_price
        }

        //==================================================================
        //                      FLASH LOANS COMPONENT
        //==================================================================

        pub fn flash_borrow(&mut self, amount: Decimal) -> (Bucket, Bucket) {
            self.badge_vault
                .authorize_with_amount(dec!("0.75"), || self.flash_loans.borrow(amount))
        }

        pub fn flash_pay_back(&mut self, receipt_bucket: Bucket, payment_bucket: Bucket) -> Bucket {
            self.badge_vault.authorize_with_amount(dec!("0.75"), || {
                self.flash_loans.pay_back(receipt_bucket, payment_bucket)
            })
        }

        pub fn flash_retrieve_interest(&mut self) -> Bucket {
            self.badge_vault
                .authorize_with_amount(dec!("0.75"), || self.flash_loans.retrieve_interest())
        }

        //==================================================================
        //                      ORACLE COMPONENT
        //==================================================================

        pub fn add_pair_to_oracle(
            &mut self,
            resource_address: ResourceAddress,
            market_id: String,
            starting_price: Decimal,
        ) {
            self.oracle.call_raw::<()>(
                "add_pair",
                scrypto_args!(resource_address, market_id, starting_price),
            );
        }
    }
}

#[derive(ScryptoSbor)]
pub struct StabPriceData {
    /// The latest price errors for the STAB token (market price - internal price), used for calculating the interest rate
    pub latest_stab_price_errors: KeyValueStore<u64, Decimal>,
    /// The total of the latest price errors
    pub latest_stab_price_errors_total: Decimal,
    /// The time of the last update
    pub last_update: Instant,
    /// The key of the last price change in the price_errors KVS
    pub last_changed_price: u64,
    /// STAB token internal price
    pub internal_price: Decimal,
    /// Whether the cache is full
    pub full_cache: bool,
    /// The interest rate for the STAB token
    pub interest_rate: Decimal,
}

#[derive(ScryptoSbor)]
pub struct InterestParameters {
    /// The Kp value for the interest rate calculation
    pub kp: Decimal,
    /// The Ki value for the interest rate calculation
    pub ki: Decimal,
    /// The maximum interest rate
    pub max_interest_rate: Decimal,
    /// The minimum interest rate
    pub min_interest_rate: Decimal,
    /// The allowed deviation for the internal price (for it to not count any price error in interest rate calculation)
    pub allowed_deviation: Decimal,
    /// The maximum price error allowed
    pub max_price_error: Decimal,
    /// The offset for the price error
    pub price_error_offset: Decimal,
}
