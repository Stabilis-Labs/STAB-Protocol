use scrypto::prelude::*;

#[blueprint]
mod dummy_token_pool {
    struct TokenPool {
        pool: Global<OneResourcePool>,
    }

    impl TokenPool {
        pub fn instantiate_token_pool(
            address: ResourceAddress,
            initial_tokens: Bucket,
        ) -> (Global<TokenPool>, Bucket, ComponentAddress) {
            let mut pool = Blueprint::<OneResourcePool>::instantiate(
                OwnerRole::None,
                rule!(allow_all),
                address,
                None,
            );

            let pool_tokens = pool.contribute(initial_tokens);
            let pool_address = pool.address();

            let component = Self { pool }
                .instantiate()
                .prepare_to_globalize(OwnerRole::None)
                .globalize();

            (component, pool_tokens, pool_address)
        }

        pub fn protected_deposit(&mut self, tokens: Bucket) {
            self.pool.protected_deposit(tokens);
        }
    }
}
