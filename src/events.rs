use scrypto::prelude::*;
use crate::shared_structs::*;

#[derive(ScryptoSbor, ScryptoEvent, Clone)]
pub struct EventAddCollateral {
    pub address: ResourceAddress,
    pub mcr: Decimal,
    pub usd_price: Decimal,
}

#[derive(ScryptoSbor, ScryptoEvent, Clone)]
pub struct EventAddPoolCollateral {
    pub address: ResourceAddress,
    pub parent_address: ResourceAddress,
}

#[derive(ScryptoSbor, ScryptoEvent, Clone)]
pub struct EventNewCdp {
    pub cdp: Cdp,
    pub cdp_id: NonFungibleLocalId,
}

#[derive(ScryptoSbor, ScryptoEvent, Clone)]
pub struct EventUpdateCdp {
    pub cdp: Cdp,
    pub cdp_id: NonFungibleLocalId,
}

#[derive(ScryptoSbor, ScryptoEvent, Clone)]
pub struct EventCloseCdp {
    pub cdp_id: NonFungibleLocalId,
}

#[derive(ScryptoSbor, ScryptoEvent, Clone)]
pub struct EventLiquidateCdp {
    pub cdp_id: NonFungibleLocalId,
}

#[derive(ScryptoSbor, ScryptoEvent, Clone)]
pub struct EventChangeCollateral {
    pub address: ResourceAddress,
    pub new_mcr: Option<Decimal>,
    pub new_usd_price: Option<Decimal>,
}

#[derive(ScryptoSbor, ScryptoEvent, Clone)]
pub struct EventChangePeg {
    pub internal_price: Decimal,
}