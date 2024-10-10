//! # Oracle Blueprint
//! Component aggregating Oracle data and processes it into data usable by the Proxy Component.

use scrypto::prelude::*;

#[derive(ScryptoSbor, Clone)]
pub struct PriceMessage {
    pub market_id: String,
    pub price: Decimal,
    pub nonce: u64,
    pub created_at: u64,
}

#[blueprint]
mod oracle {
    enable_method_auth! {
        methods {
            get_prices => PUBLIC;
            set_price => PUBLIC;
            add_pair => restrict_to: [OWNER];
        }
    }

    extern_blueprint! {
        "package_tdx_2_1phdppf684w8r4za9pwgafzc0zpmsvt7xlmyx8r7kzq2dlgns9k5war", //stokenet morpher package
        //package_rdx1p5xvvessslnpnfam9weyzldlxr7q06gen2t3d3waa0x760g7jwxhkd, //mainnet morpher package
        MorpherOracle {
            fn check_price_input(&self, message: String, signature: String) -> PriceMessage;
        }
    }

    struct Oracle {
        prices: Vec<(ResourceAddress, Decimal, u64, String)>,
        oracle_address: ComponentAddress,
    }

    impl Oracle {
        pub fn instantiate_oracle(
            owner_role: OwnerRole,
            oracle_address: ComponentAddress,
            dapp_def_address: GlobalAddress,
        ) -> Global<Oracle> {
            let prices: Vec<(ResourceAddress, Decimal, u64, String)> = vec![(
                XRD,
                dec!("0.015"),
                Clock::current_time_rounded_to_seconds().seconds_since_unix_epoch as u64,
                "GATEIO:XRD_USDT".to_string(),
            )];

            Self {
                prices,
                oracle_address,
            }
            .instantiate()
            .prepare_to_globalize(owner_role)
            .metadata(metadata! {
                init {
                    "name" => "STAB Oracle".to_string(), updatable;
                    "description" => "An oracle used to keep track of collateral prices for STAB".to_string(), updatable;
                    "info_url" => Url::of("https://ilikeitstable.com"), updatable;
                    "dapp_definition" => dapp_def_address, updatable;
                }
            })
            .globalize()
        }

        pub fn get_prices(&mut self) -> Vec<(ResourceAddress, Decimal, u64, String)> {
            self.prices.clone()
        }

        //manual price setting, not necessary after religant is available and part in get_prices can be uncommented
        pub fn set_price(&mut self, message: String, signature: String) {
            let morpher_oracle = Global::<MorpherOracle>::from(self.oracle_address);
            let price_message = morpher_oracle.check_price_input(message, signature);

            for prices in self.prices.iter_mut() {
                if prices.3 == price_message.market_id {
                    assert!(price_message.created_at > prices.2, "Price is too old");
                    prices.1 = price_message.price;
                    prices.2 = price_message.created_at;
                }
            }
        }

        pub fn add_pair(
            &mut self,
            resource_address: ResourceAddress,
            market_id: String,
            starting_price: Decimal,
        ) {
            self.prices.push((
                resource_address,
                starting_price,
                Clock::current_time_rounded_to_seconds().seconds_since_unix_epoch as u64,
                market_id,
            ));
        }
    }
}
