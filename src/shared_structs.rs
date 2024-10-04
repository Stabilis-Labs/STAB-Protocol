//! # Proxy and STAB Blueprint shared structs
//! Structs used by both the Proxy and STAB component

use scrypto::prelude::*;

/// Data struct of a loan receipt / CDP receipt, gained when opening a CDP / loan
#[derive(ScryptoSbor, NonFungibleData)]
pub struct Cdp {
    /// collateral used for this loan / cdp
    pub collateral: ResourceAddress,
    /// parent address of this collateral (only differs from collateral in the case of a pool unit)
    pub parent_address: ResourceAddress,
    /// whether collateral is a pool unit
    pub is_pool_unit_collateral: bool,

    /// amount of collateral used
    #[mutable]
    pub collateral_amount: Decimal,
    /// amount of stab minted
    #[mutable]
    pub minted_stab: Decimal,
    /// real_collateral_amount / minted_stab
    /// where real_collateral_amount is the amount of parent_address collateral
    ///     - which is the same as collateral_amount for non pool unit collaterals
    ///     - which is different than the collateral_amount for pool units, for instance an LSU that's worth 1.1 XRD per LSU, will have a real_collateral_amount 1.1 times the collateral_amount
    #[mutable]
    pub collateral_stab_ratio: Decimal,
    /// status of the cdp / loan
    #[mutable]
    pub status: CdpStatus,
    /// id of the marker that last marked this loan
    #[mutable]
    pub marker_id: u64,
}

/// Data struct of a CDP Marker, gained when marking a loan / CDP for liquidation
#[derive(ScryptoSbor, NonFungibleData)]
pub struct CdpMarker {
    /// Type of marking
    pub mark_type: CdpUpdate,
    /// Time of marking
    pub time_marked: Instant,
    /// ID of marked CDP / loan
    pub marked_id: NonFungibleLocalId,
    /// Marker ID in AvlTree storing markers
    pub marker_placing: Decimal,

    ///whether the marker has been used
    #[mutable]
    pub used: bool,
}

///Data of Liquidation Receipt, gained when liquidating a loan
#[derive(ScryptoSbor, NonFungibleData)]
pub struct LiquidationReceipt {
    /// collateral used to liquidate
    pub collateral: ResourceAddress,
    /// stab paid to liquidate
    pub stab_paid: Decimal,
    /// percentage of the stab value received as collateral (example: if liq reward is 0.1, this would probably be 1.1, but if cr < 1.1, this would be cr)
    pub percentage_received: Decimal,
    /// percentage liquidator should've received at time of liquidation (example: if liq reward is 0.1, this should be 1.1)
    pub percentage_owed: Decimal,
    /// the id of the liquidated CDP / loan
    pub cdp_liquidated: NonFungibleLocalId,
    /// time of liquidation
    pub date_liquidated: Instant,
}

/// Status of a CDP
#[derive(ScryptoSbor, PartialEq)]
pub enum CdpStatus {
    Healthy,
    Marked,
    Liquidated,
    ForceLiquidated,
    Closed,
}

/// The kind of update that the action has executed.
#[derive(ScryptoSbor, PartialEq)]
pub enum CdpUpdate {
    Marked,
    Saved,
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
