//! # Oracle Blueprint
//! Component aggregating Oracle data and processes it into data usable by the Proxy Component.

use scrypto::prelude::*;

/*#[derive(ScryptoSbor, PartialEq, Eq, PartialOrd, Ord, Debug, Copy, Clone)]
pub struct PriceData {
    pub price: Decimal,
    pub timestamp: i64,
}*/

#[blueprint]
mod oracle {
    // not necessary after religant is available
    enable_method_auth! {
        methods {
            get_prices => PUBLIC;
            set_xrd_price => restrict_to: [OWNER];
        }
    }

    // Can be uncommented if the Religant component is available again
    /*const RELIGANT: Global<Religant> = global_component!(
        Religant,
        "component_tdx_2_1cpekt6s65g8025zgstwx4t0tpdsegafse0vtjnfms9k07mcmnr96cm"
    );

    extern_blueprint! {
        "package_tdx_2_1p50j7463yhtpmq8e9t4vklw8jfuccl0xhe7g2564w8w74nrmrsacxs",
        Religant {
            fn get_price(&self) -> Option<PriceData>;
        }
    }*/

    struct Oracle {
        prices: Vec<(ResourceAddress, Decimal)>,
        //religant: Global<Religant>,
    }

    impl Oracle {
        pub fn instantiate_oracle(controller: ResourceAddress) -> Global<Oracle> {
            let prices: Vec<(ResourceAddress, Decimal)> = vec![(XRD, dec!("0.041"))];

            Self {
                prices,
                //religant: RELIGANT,
            }
            .instantiate()
            .prepare_to_globalize(OwnerRole::Fixed(rule!(require(
                controller
            ))))
            .globalize()
        }

        pub fn get_prices(&mut self) -> Vec<(ResourceAddress, Decimal)> {
            /*if let Some(xrd_price_info) = self.religant.get_price() {
                if xrd_price_info.price != self.prices[0].1 {
                    self.prices[0].1 = xrd_price_info.price;
                }
            }*/
            self.prices.clone()
        }

        //manual price setting, not necessary after religant is available and part in get_prices can be uncommented
        pub fn set_xrd_price(&mut self, price: Decimal) {
            self.prices[0].1 = price;
        }
    }
}
