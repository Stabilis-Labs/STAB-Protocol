//! # Stabilis Liquidity Pool Blueprint
//!
//! This blueprint instantiates a liquidity pool for the Stabilis protocol. The pool is a native STAB/XRD liquidity pool, and is used to determine the price of STAB tokens.

use scrypto::prelude::*;

#[blueprint]
mod stabilis_liquidity_pool {
    enable_method_auth! {
        methods {
            add_liquidity => PUBLIC;
            remove_liquidity => PUBLIC;
            get_stab_price => PUBLIC;
            swap => PUBLIC;
            set_fee => restrict_to: [OWNER];
        }
    }

    struct StabilisPool {
        /// The global instance of the TwoResourcePool component, holding the STAB/XRD liquidity pool
        pool_component: Global<TwoResourcePool>,
        /// The fee charged for swaps
        fee: Decimal,
    }

    impl StabilisPool {
        /// Instantiates the StabilisPool component
        ///
        /// # Input
        /// - `owner_role`: The owner role of the StabilisPool component
        /// - `resource_address1`: The address of the first resource in the pool
        /// - `resource_address2`: The address of the second resource in the pool
        /// - `fee`: The fee charged for swaps
        ///
        /// # Output
        /// - The global instance of the StabilisPool component
        ///
        /// # Logic
        /// - Instantiates the TwoResourcePool component
        /// - Instantiates the StabilisPool component
        pub fn new(
            owner_role: OwnerRole, //proxy owner badge
            resource_address1: ResourceAddress,
            resource_address2: ResourceAddress,
            fee: Decimal,
        ) -> Global<StabilisPool> {
            let (address_reservation, component_address) =
                Runtime::allocate_component_address(StabilisPool::blueprint_id());
            let global_component_caller_badge =
                NonFungibleGlobalId::global_caller_badge(component_address);

            let pool_component = Blueprint::<TwoResourcePool>::instantiate(
                owner_role.clone(),
                rule!(require(global_component_caller_badge)),
                (resource_address1, resource_address2),
                None,
            );

            Self {
                pool_component,
                fee,
            }
            .instantiate()
            .prepare_to_globalize(owner_role)
            .with_address(address_reservation)
            .globalize()
        }

        /// Adds liquidity to the pool
        ///
        /// # Input
        /// - `resource1`: The first resource to add to the pool
        /// - `resource2`: The second resource to add to the pool
        ///
        /// # Output
        /// - The pool units received
        /// - The leftover resource, if any
        ///
        /// # Logic
        /// - Contributes the resources to the pool and returns them
        pub fn add_liquidity(
            &mut self,
            resource1: Bucket,
            resource2: Bucket,
        ) -> (Bucket, Option<Bucket>) {
            self.pool_component.contribute((resource1, resource2))
        }

        /// Removes liquidity from the pool
        ///
        /// # Input
        /// - `pool_units`: The pool units to remove
        ///
        /// # Output
        /// - The resource1 received
        /// - The resource2 received
        ///
        /// # Logic
        /// - Redeems and returns the pool units
        pub fn remove_liquidity(&mut self, pool_units: Bucket) -> (Bucket, Bucket) {
            self.pool_component.redeem(pool_units)
        }

        /// Swaps one resource for another
        ///
        /// # Input
        /// - `input_bucket`: The bucket to swap
        ///
        /// # Output
        /// - The resulting tokens
        ///
        /// # Logic
        /// - Checks the token reserves for the pool
        /// - Calculates the output amount for the input amount
        /// - Deposits the input bucket
        /// - Withdraws and returns the output bucket
        pub fn swap(&mut self, input_bucket: Bucket) -> Bucket {
            let mut reserves = self.vault_reserves();

            let input_reserves = reserves
                .swap_remove(&input_bucket.resource_address())
                .expect("Resource does not belong to the pool");
            let (output_resource_address, output_reserves) = reserves.into_iter().next().unwrap();

            let input_amount = input_bucket.amount();

            let output_amount = (input_amount * output_reserves * (dec!("1") - self.fee))
                / (input_reserves + input_amount * (dec!("1") - self.fee));

            self.deposit(input_bucket);

            self.withdraw(output_resource_address, output_amount)
        }

        /// Gets the price of STAB tokens (or, if you've instantiated a different pool, the price of the first resource in the pool)
        ///
        /// # Output
        /// - The price of STAB tokens (in XRD)
        ///
        /// # Logic
        /// - Gets amount of both resources in the pool
        /// - Returns the price by dividing amounts
        pub fn get_stab_price(&self) -> Decimal {
            let reserves = self.vault_reserves();
            let first_amount: Decimal = *reserves.first().map(|(_, v)| v).unwrap();
            let last_amount: Decimal = *reserves.last().map(|(_, v)| v).unwrap();
            last_amount / first_amount
        }

        /// Sets the fee charged for swaps
        pub fn set_fee(&mut self, fee: Decimal) {
            self.fee = fee;
        }

        /// Gets the reserves of the pool
        fn vault_reserves(&self) -> IndexMap<ResourceAddress, Decimal> {
            self.pool_component.get_vault_amounts()
        }

        /// Deposits a bucket into the pool (using the TwoResourcePool component's logic)
        fn deposit(&mut self, bucket: Bucket) {
            self.pool_component.protected_deposit(bucket)
        }

        /// Withdraws a bucket from the pool (using the TwoResourcePool component's logic)
        fn withdraw(&mut self, resource_address: ResourceAddress, amount: Decimal) -> Bucket {
            self.pool_component.protected_withdraw(
                resource_address,
                amount,
                WithdrawStrategy::Rounded(RoundingMode::ToZero),
            )
        }
    }
}
