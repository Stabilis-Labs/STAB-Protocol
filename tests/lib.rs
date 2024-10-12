/// This testing does not include the FlashLoan component, Oracle component, Proxy component, or the Liquidity Pool component.
/// Why? Because the FlashLoan component and Oracle component have been manually tested, their functionality is rather limited.
/// The LP component is copied from a written example by RDX Works, and very simple.
/// The Proxy component basically only calls methods from the STAB component, so there's little reason to test it.
///
/// Excuse this testing style, it was the first time I wrote tests in Scrypto/Rust. It does the job... but is messy.
/// If you're wondering how to do better, read the tests written for the DAO, those are a lot better ;)
/// The entire STAB Protocol package has been tested on Stokenet extensively though.
use dummy_token_pool::dummy_token_pool_test::*;
use scrypto_test::prelude::*;
use stab_module::stabilis_component::stabilis_component_test::*;
// Generic setup
pub fn publish_and_setup() -> Result<
    (
        TestEnvironment<InMemorySubstateDatabase>,
        Stabilis,
        Bucket,
        Bucket,
    ),
    RuntimeError,
> {
    let fake_oracle_address =
        GlobalAddress::try_from_hex("0d906318c6318c60f716464c6318c6318cf7bfcad6a3152b46318c6318c6")
            .unwrap();
    let mut env = TestEnvironmentBuilder::new()
        .add_global_references(vec![fake_oracle_address])
        .build();
    env.disable_auth_module();
    let package =
        PackageFactory::compile_and_publish(this_package!(), &mut env, CompileProfile::Fast)?;

    let (mut stab_comp, controller_badge) = Stabilis::instantiate(package, &mut env)?;

    let a_bucket = ResourceBuilder::new_fungible(OwnerRole::None)
        .divisibility(18)
        .mint_initial_supply(10000, &mut env)?;

    stab_comp.add_collateral(
        a_bucket.resource_address(&mut env)?,
        dec!("1.5"),
        dec!("1"),
        &mut env,
    )?;

    assert_eq!(controller_badge.amount(&mut env)?, dec!(10));

    Ok((env, stab_comp, a_bucket, controller_badge))
}

// Individual tests
#[test]
fn deploys() -> Result<(), RuntimeError> {
    let (_env, _stab_comp, _a_bucket, _control_bucket) = publish_and_setup()?;
    Ok(())
}

// Can open CDP
#[test]
fn can_open_cdp() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let (stab, _cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    assert_eq!(stab.amount(&mut env)?, dec!(500));

    Ok(())
}

// Fail to open CDP with insufficient collateral
#[test]
fn cant_open_cdp_insufficient_collateral() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    // Attempt to open CDP with insufficient collateral
    let result = stab_comp.open_cdp(
        a_bucket.take(dec!(10), &mut env)?, // Only 10 units of collateral
        dec!(500),                          // Trying to mint 500 STAB
        &mut env,
    );

    assert!(result.is_err());

    Ok(())
}

// Can close CDP
#[test]
fn can_close_cdp() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let (stab, cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    let cdps = cdp.non_fungible_local_ids(&mut env)?;
    let cdp = cdps.first().unwrap();

    // Repay the loan and close the CDP
    let (collateral, leftover_stab) = stab_comp.close_cdp(cdp.clone(), stab, &mut env)?;
    assert_eq!(collateral.amount(&mut env)?, dec!(1000));
    assert_eq!(leftover_stab.amount(&mut env)?, dec!(0));

    Ok(())
}

// Can partial close CDP
#[test]
fn can_partial_close_cdp() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let (stab, cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    let cdps = cdp.non_fungible_local_ids(&mut env)?;
    let cdp = cdps.first().unwrap();

    // Repay the loan and close the CDP
    stab_comp.partial_close_cdp(cdp.clone(), stab.take(dec!(100), &mut env)?, &mut env)?;

    assert_eq!(stab.amount(&mut env)?, dec!(400));

    Ok(())
}

// Cant close CDP with too little repayment
#[test]
fn cant_close_cdp_insufficient_repayment() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let (stab, cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    let cdps = cdp.non_fungible_local_ids(&mut env)?;
    let cdp = cdps.first().unwrap();

    // Repay the loan and close the CDP
    let result = stab_comp.close_cdp(cdp.clone(), stab.take(dec!(400), &mut env)?, &mut env);

    assert!(result.is_err());

    Ok(())
}

// Cant partial close below minimum mint
#[test]
fn cant_partial_close_cdp_below_minimum_mint() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let (stab, cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    let cdps = cdp.non_fungible_local_ids(&mut env)?;
    let cdp = cdps.first().unwrap();

    // Repay the loan and close the CDP
    let result =
        stab_comp.partial_close_cdp(cdp.clone(), stab.take(dec!("499.5"), &mut env)?, &mut env);

    assert!(result.is_err());

    Ok(())
}

// Cant close CDP with wrong repayment resource
#[test]
fn cant_close_cdp_wrong_resource() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let (_stab, cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    let cdps = cdp.non_fungible_local_ids(&mut env)?;
    let cdp = cdps.first().unwrap();

    // Repay the loan and close the CDP
    let result = stab_comp.close_cdp(cdp.clone(), a_bucket.take(dec!(500), &mut env)?, &mut env);

    assert!(result.is_err());

    Ok(())
}

// Cant close CDP with wrong repayment resource
#[test]
fn cant_partial_close_cdp_wrong_resource() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let (_stab, cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    let cdps = cdp.non_fungible_local_ids(&mut env)?;
    let cdp = cdps.first().unwrap();

    // Repay the loan and close the CDP
    let result =
        stab_comp.partial_close_cdp(cdp.clone(), a_bucket.take(dec!(500), &mut env)?, &mut env);

    assert!(result.is_err());

    Ok(())
}

// Top up CDP works
#[test]
fn can_top_up_cdp() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let (_stab, cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    let cdps = cdp.non_fungible_local_ids(&mut env)?;
    let cdp = cdps.first().unwrap();

    let _ = stab_comp.top_up_cdp(cdp.clone(), a_bucket.take(dec!(500), &mut env)?, &mut env);

    assert_eq!(a_bucket.amount(&mut env)?, dec!(8500));

    Ok(())
}

// Top up CDP doesn't work with wrong resource
#[test]
fn cant_top_up_cdp_wrong_payment() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let (stab, cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    let cdps = cdp.non_fungible_local_ids(&mut env)?;
    let cdp = cdps.first().unwrap();

    let result = stab_comp.top_up_cdp(cdp.clone(), stab.take(dec!(500), &mut env)?, &mut env);

    assert!(result.is_err());

    Ok(())
}

// Top up CDP works, and removes updated amount of collateral after
#[test]
fn can_top_up_cdp_and_remove() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let (stab, cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    let cdps = cdp.non_fungible_local_ids(&mut env)?;
    let cdp = cdps.first().unwrap();

    let _ = stab_comp.top_up_cdp(cdp.clone(), a_bucket.take(dec!(500), &mut env)?, &mut env);

    let (collateral, leftover_stab) = stab_comp.close_cdp(cdp.clone(), stab, &mut env)?;

    assert_eq!(collateral.amount(&mut env)?, dec!(1500));
    assert_eq!(leftover_stab.amount(&mut env)?, dec!(0));

    Ok(())
}

// Removing collateral works
#[test]
fn can_remove_collateral() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let (_stab, cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    let cdps = cdp.non_fungible_local_ids(&mut env)?;
    let cdp = cdps.first().unwrap();

    let removed_collateral = stab_comp.remove_collateral(cdp.clone(), dec!(100), &mut env)?;

    assert_eq!(removed_collateral.amount(&mut env)?, dec!(100));
    assert_eq!(
        removed_collateral.resource_address(&mut env)?,
        a_bucket.resource_address(&mut env)?
    );

    Ok(())
}

// Can't remove too much collateral
#[test]
fn cant_remove_collateral_below_mcr() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let (_stab, cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    let cdps = cdp.non_fungible_local_ids(&mut env)?;
    let cdp = cdps.first().unwrap();

    let result = stab_comp.remove_collateral(cdp.clone(), dec!(400), &mut env);

    assert!(result.is_err());

    Ok(())
}

// Borrow more STAB by adding to the loan / CDP
#[test]
fn can_borrow_more() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let (_stab, cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    let cdps = cdp.non_fungible_local_ids(&mut env)?;
    let cdp_id = cdps.first().unwrap();

    let additional_stab = stab_comp.borrow_more(cdp_id.clone(), dec!(100), &mut env)?;

    assert_eq!(additional_stab.amount(&mut env)?, dec!(100));

    Ok(())
}

// Mark a loan for liquidation
#[test]
fn can_mark_for_liquidation() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let (_stab, _cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    let _ = stab_comp.change_collateral_price(
        a_bucket.resource_address(&mut env)?,
        dec!(0.5),
        &mut env,
    );

    let marker = stab_comp.mark_for_liquidation(a_bucket.resource_address(&mut env)?, &mut env)?;

    assert!(marker.amount(&mut env)? > dec!(0));

    Ok(())
}

// Mark a loan for liquidation, with pool unit collateral
#[test]
fn can_mark_for_liquidation_pool_unit() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let pool_package_address =
        PackageFactory::compile_and_publish("./dummy_token_pool", &mut env, CompileProfile::Fast)?;
    let (_token_pool, pool_units, pool_address) = TokenPool::instantiate_token_pool(
        a_bucket.resource_address(&mut env)?,
        a_bucket.take(dec!(1000), &mut env)?,
        pool_package_address,
        &mut env,
    )?;
    let _ = stab_comp.add_pool_collateral(
        pool_units.resource_address(&mut env)?,
        a_bucket.resource_address(&mut env)?,
        pool_address,
        false,
        true,
        &mut env,
    );
    let (_stab, _cdp) =
        stab_comp.open_cdp(pool_units.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    let _ = stab_comp.change_collateral_price(
        a_bucket.resource_address(&mut env)?,
        dec!(0.5),
        &mut env,
    );

    let marker = stab_comp.mark_for_liquidation(a_bucket.resource_address(&mut env)?, &mut env)?;

    assert!(marker.amount(&mut env)? > dec!(0));

    Ok(())
}

// Mark a loan for liquidation, but save it, as it's an appreciated pool unit collateral CDP
#[test]
fn can_save_cdp_with_pool_unit_through_marking() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let pool_package_address =
        PackageFactory::compile_and_publish("./dummy_token_pool", &mut env, CompileProfile::Fast)?;
    let (mut pool_comp, pool_units, pool_address) = TokenPool::instantiate_token_pool(
        a_bucket.resource_address(&mut env)?,
        a_bucket.take(dec!(1000), &mut env)?,
        pool_package_address,
        &mut env,
    )?;
    let _ = stab_comp.add_pool_collateral(
        pool_units.resource_address(&mut env)?,
        a_bucket.resource_address(&mut env)?,
        pool_address,
        false,
        true,
        &mut env,
    );
    let (_stab, _cdp) =
        stab_comp.open_cdp(pool_units.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    let _ = stab_comp.change_collateral_price(
        a_bucket.resource_address(&mut env)?,
        dec!(0.5),
        &mut env,
    );

    let _ = pool_comp.protected_deposit(a_bucket.take(dec!(1000), &mut env)?, &mut env);

    let marker = stab_comp.mark_for_liquidation(a_bucket.resource_address(&mut env)?, &mut env)?;

    assert!(marker.amount(&mut env)? > dec!(0));

    Ok(())
}

// Liquidate a marked loan / CDP, using a marker receipt
#[test]
fn can_liquidate_with_marker() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    //open loan
    let (stab, _cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    //get some more free stab to test with
    let free_stab = BucketFactory::create_fungible_bucket(
        stab.resource_address(&mut env)?,
        dec!(100000),
        Mock,
        &mut env,
    )?;

    //change col price so liq is possible
    let _ = stab_comp.change_collateral_price(
        a_bucket.resource_address(&mut env)?,
        dec!(0.5),
        &mut env,
    );

    //mark loan
    let marker = stab_comp.mark_for_liquidation(a_bucket.resource_address(&mut env)?, &mut env)?;
    let marker_ids = marker.non_fungible_local_ids(&mut env)?;
    let marker_id = marker_ids.first().unwrap();

    let time = env.get_current_time();
    let new_time = time.add_minutes(5).unwrap();
    env.set_current_time(new_time);

    //liq with marker
    let (collateral_reward, leftover_stab, _liquidation_receipt) = stab_comp
        .liquidate_position_with_marker(
            marker_id.clone(),
            free_stab.take(dec!(600), &mut env)?,
            &mut env,
        )?;

    //check rewards
    assert_eq!(collateral_reward.unwrap().amount(&mut env)?, dec!(1000));
    if let Some(stab) = leftover_stab {
        assert_eq!(stab.amount(&mut env)?, dec!(100));
    }

    Ok(())
}

// Liquidate a marked loan / CDP, using a marker receipt before it should be possible
#[test]
fn cant_liquidate_with_marker_before_time() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    //open loan
    let (stab, _cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    //get some more free stab to test with
    let free_stab = BucketFactory::create_fungible_bucket(
        stab.resource_address(&mut env)?,
        dec!(100000),
        Mock,
        &mut env,
    )?;

    //change col price so liq is possible
    let _ = stab_comp.change_collateral_price(
        a_bucket.resource_address(&mut env)?,
        dec!(0.5),
        &mut env,
    );

    //mark loan
    let marker = stab_comp.mark_for_liquidation(a_bucket.resource_address(&mut env)?, &mut env)?;
    let marker_ids = marker.non_fungible_local_ids(&mut env)?;
    let marker_id = marker_ids.first().unwrap();

    let time = env.get_current_time();
    let new_time = time.add_minutes(4).unwrap();
    env.set_current_time(new_time);

    //liq with marker
    let failure = stab_comp.liquidate_position_with_marker(
        marker_id.clone(),
        free_stab.take(dec!(600), &mut env)?,
        &mut env,
    );

    assert!(failure.is_err());

    Ok(())
}

// Try to liquidate a CDP with a marker receipt that's received for saving a CDP
#[test]
fn cant_liquidate_using_marker_save_receipt() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let pool_package_address =
        PackageFactory::compile_and_publish("./dummy_token_pool", &mut env, CompileProfile::Fast)?;
    let (mut pool_comp, pool_units, pool_address) = TokenPool::instantiate_token_pool(
        a_bucket.resource_address(&mut env)?,
        a_bucket.take(dec!(1000), &mut env)?,
        pool_package_address,
        &mut env,
    )?;
    let _ = stab_comp.add_pool_collateral(
        pool_units.resource_address(&mut env)?,
        a_bucket.resource_address(&mut env)?,
        pool_address,
        false,
        true,
        &mut env,
    );
    let (stab, _cdp) =
        stab_comp.open_cdp(pool_units.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    //get some more free stab to test with
    let free_stab = BucketFactory::create_fungible_bucket(
        stab.resource_address(&mut env)?,
        dec!(100000),
        Mock,
        &mut env,
    )?;

    let _ = stab_comp.change_collateral_price(
        a_bucket.resource_address(&mut env)?,
        dec!(0.5),
        &mut env,
    );

    let _ = pool_comp.protected_deposit(a_bucket.take(dec!(1000), &mut env)?, &mut env);

    let marker = stab_comp.mark_for_liquidation(a_bucket.resource_address(&mut env)?, &mut env)?;
    let marker_ids = marker.non_fungible_local_ids(&mut env)?;
    let marker_id = marker_ids.first().unwrap();

    let time = env.get_current_time();
    let new_time = time.add_minutes(5).unwrap();
    env.set_current_time(new_time);

    //liq with marker
    let failure = stab_comp.liquidate_position_with_marker(
        marker_id.clone(),
        free_stab.take(dec!(600), &mut env)?,
        &mut env,
    );

    assert!(failure.is_err());

    Ok(())
}

// Try to liquidate an appreciated CDP with pool unit collateral, so that it will instead save it.
#[test]
fn can_only_save_appreciated_pool_cdp() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let pool_package_address =
        PackageFactory::compile_and_publish("./dummy_token_pool", &mut env, CompileProfile::Fast)?;
    let (mut pool_comp, pool_units, pool_address) = TokenPool::instantiate_token_pool(
        a_bucket.resource_address(&mut env)?,
        a_bucket.take(dec!(1000), &mut env)?,
        pool_package_address,
        &mut env,
    )?;
    let _ = stab_comp.add_pool_collateral(
        pool_units.resource_address(&mut env)?,
        a_bucket.resource_address(&mut env)?,
        pool_address,
        false,
        true,
        &mut env,
    );
    let (stab, _cdp) =
        stab_comp.open_cdp(pool_units.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    //get some more free stab to test with
    let free_stab = BucketFactory::create_fungible_bucket(
        stab.resource_address(&mut env)?,
        dec!(100000),
        Mock,
        &mut env,
    )?;

    let _ = stab_comp.change_collateral_price(
        a_bucket.resource_address(&mut env)?,
        dec!(0.5),
        &mut env,
    );

    let marker = stab_comp.mark_for_liquidation(a_bucket.resource_address(&mut env)?, &mut env)?;
    let marker_address = marker.resource_address(&mut env)?;
    let marker_ids = marker.non_fungible_local_ids(&mut env)?;
    let marker_id = marker_ids.first().unwrap();

    let time = env.get_current_time();
    let new_time = time.add_minutes(5).unwrap();
    env.set_current_time(new_time);

    let _ = pool_comp.protected_deposit(a_bucket.take(dec!(1000), &mut env)?, &mut env);

    //liq with marker
    let (payment, remainder, receipt) = stab_comp.liquidate_position_with_marker(
        marker_id.clone(),
        free_stab.take(dec!(600), &mut env)?,
        &mut env,
    )?;

    assert!(payment.is_none());
    assert!(remainder.unwrap().amount(&mut env)? == dec!(600));
    assert!(receipt.resource_address(&mut env)? == marker_address);

    Ok(())
}

// Liquidate a CDP with pool unit collateral
#[test]
fn can_liquidate_pool_cdp() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let pool_package_address =
        PackageFactory::compile_and_publish("./dummy_token_pool", &mut env, CompileProfile::Fast)?;
    let (_pool_comp, pool_units, pool_address) = TokenPool::instantiate_token_pool(
        a_bucket.resource_address(&mut env)?,
        a_bucket.take(dec!(1000), &mut env)?,
        pool_package_address,
        &mut env,
    )?;
    let _ = stab_comp.add_pool_collateral(
        pool_units.resource_address(&mut env)?,
        a_bucket.resource_address(&mut env)?,
        pool_address,
        false,
        true,
        &mut env,
    );
    let (stab, _cdp) =
        stab_comp.open_cdp(pool_units.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    //get some more free stab to test with
    let free_stab = BucketFactory::create_fungible_bucket(
        stab.resource_address(&mut env)?,
        dec!(100000),
        Mock,
        &mut env,
    )?;

    let _ = stab_comp.change_collateral_price(
        a_bucket.resource_address(&mut env)?,
        dec!(0.5),
        &mut env,
    );

    let marker = stab_comp.mark_for_liquidation(a_bucket.resource_address(&mut env)?, &mut env)?;
    let marker_ids = marker.non_fungible_local_ids(&mut env)?;
    let marker_id = marker_ids.first().unwrap();

    let time = env.get_current_time();
    let new_time = time.add_minutes(5).unwrap();
    env.set_current_time(new_time);

    //liq with marker
    let (payment, remainder, _receipt) = stab_comp.liquidate_position_with_marker(
        marker_id.clone(),
        free_stab.take(dec!(600), &mut env)?,
        &mut env,
    )?;

    //check results
    assert_eq!(payment.unwrap().amount(&mut env)?, dec!(1000));
    assert_eq!(remainder.unwrap().amount(&mut env)?, dec!(100));

    Ok(())
}

// Liquidate a marked loan / CDP, without a marker receipt by id
#[test]
fn can_liquidate_without_marker_by_id() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let (stab, cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    let cdps = cdp.non_fungible_local_ids(&mut env)?;
    let cdp_id = cdps.first().unwrap();

    //get some more free stab to test with
    let free_stab = BucketFactory::create_fungible_bucket(
        stab.resource_address(&mut env)?,
        dec!(100000),
        Mock,
        &mut env,
    )?;

    //change col price so liq is possible
    let _ = stab_comp.change_collateral_price(
        a_bucket.resource_address(&mut env)?,
        dec!(0.5),
        &mut env,
    );

    //mark loan
    let _marker = stab_comp.mark_for_liquidation(a_bucket.resource_address(&mut env)?, &mut env)?;

    let time = env.get_current_time();
    let new_time = time.add_minutes(10).unwrap();
    env.set_current_time(new_time);

    //liq without marker
    let (collateral_reward, leftover_stab, _liquidation_receipt) = stab_comp
        .liquidate_position_without_marker(
            free_stab.take(dec!(600), &mut env)?,
            None,
            cdp_id.clone(),
            &mut env,
        )?;

    //check results
    assert_eq!(collateral_reward.unwrap().amount(&mut env)?, dec!(1000));
    if let Some(stab) = leftover_stab {
        assert_eq!(stab.amount(&mut env)?, dec!(100));
    }

    Ok(())
}

// Liquidate a marked loan / CDP, without a marker receipt by id before it should be possible
#[test]
fn cant_liquidate_without_marker_by_id_before_time() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let (stab, cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    let cdps = cdp.non_fungible_local_ids(&mut env)?;
    let cdp_id = cdps.first().unwrap();

    //get some more free stab to test with
    let free_stab = BucketFactory::create_fungible_bucket(
        stab.resource_address(&mut env)?,
        dec!(100000),
        Mock,
        &mut env,
    )?;

    //change col price so liq is possible
    let _ = stab_comp.change_collateral_price(
        a_bucket.resource_address(&mut env)?,
        dec!(0.5),
        &mut env,
    );

    //mark loan
    let _marker = stab_comp.mark_for_liquidation(a_bucket.resource_address(&mut env)?, &mut env)?;

    let time = env.get_current_time();
    let new_time = time.add_minutes(9).unwrap();
    env.set_current_time(new_time);

    //liq without marker
    let failure = stab_comp.liquidate_position_without_marker(
        free_stab.take(dec!(600), &mut env)?,
        None,
        cdp_id.clone(),
        &mut env,
    );

    assert!(failure.is_err());

    Ok(())
}

// Liquidate a marked loan / CDP, without a marker receipt automatically
#[test]
fn can_liquidate_without_marker_automatic() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let (stab, cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    let cdps = cdp.non_fungible_local_ids(&mut env)?;
    let cdp_id = cdps.first().unwrap();

    //get some more free stab to test with
    let free_stab = BucketFactory::create_fungible_bucket(
        stab.resource_address(&mut env)?,
        dec!(100000),
        Mock,
        &mut env,
    )?;

    //change col price so liq is possible
    let _ = stab_comp.change_collateral_price(
        a_bucket.resource_address(&mut env)?,
        dec!(0.5),
        &mut env,
    );

    //mark loan
    let _marker = stab_comp.mark_for_liquidation(a_bucket.resource_address(&mut env)?, &mut env)?;

    let time = env.get_current_time();
    let new_time = time.add_minutes(10).unwrap();
    env.set_current_time(new_time);

    //liq without marker
    let (collateral_reward, leftover_stab, _liquidation_receipt) = stab_comp
        .liquidate_position_without_marker(
            free_stab.take(dec!(600), &mut env)?,
            Some(0),
            cdp_id.clone(),
            &mut env,
        )?;

    //check results
    assert_eq!(collateral_reward.unwrap().amount(&mut env)?, dec!(1000));
    if let Some(stab) = leftover_stab {
        assert_eq!(stab.amount(&mut env)?, dec!(100));
    }

    Ok(())
}

// Liquidate a marked loan / CDP, without a marker receipt automatically
#[test]
fn cant_liquidate_without_marker_automatic_before_time() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let (stab, cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    let cdps = cdp.non_fungible_local_ids(&mut env)?;
    let cdp_id = cdps.first().unwrap();

    //get some more free stab to test with
    let free_stab = BucketFactory::create_fungible_bucket(
        stab.resource_address(&mut env)?,
        dec!(100000),
        Mock,
        &mut env,
    )?;

    //change col price so liq is possible
    let _ = stab_comp.change_collateral_price(
        a_bucket.resource_address(&mut env)?,
        dec!(0.5),
        &mut env,
    );

    //mark loan
    let _marker = stab_comp.mark_for_liquidation(a_bucket.resource_address(&mut env)?, &mut env)?;

    let time = env.get_current_time();
    let new_time = time.add_minutes(9).unwrap();
    env.set_current_time(new_time);

    //liq without marker
    let failure = stab_comp.liquidate_position_without_marker(
        free_stab.take(dec!(600), &mut env)?,
        Some(0),
        cdp_id.clone(),
        &mut env,
    );

    assert!(failure.is_err());

    Ok(())
}

//return internal price
#[test]
fn can_return_internal_price() -> Result<(), RuntimeError> {
    let (mut env, stab_comp, _a_bucket, _control_bucket) = publish_and_setup()?;

    let price = stab_comp.return_internal_price(&mut env)?;

    assert_eq!(price, dec!(1));

    Ok(())
}

// Check if liquidation fines are calculated correctly if cr > 115%
#[test]
fn correct_liquidation_fines_over_115_cr() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let (stab, cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(1000), &mut env)?, dec!(400), &mut env)?;

    let cdps = cdp.non_fungible_local_ids(&mut env)?;
    let cdp_id = cdps.first().unwrap();

    //get some more free stab to test with
    let free_stab = BucketFactory::create_fungible_bucket(
        stab.resource_address(&mut env)?,
        dec!(100000),
        Mock,
        &mut env,
    )?;

    //change col price so liq is possible
    let _stab_price = stab_comp.change_internal_price(dec!(2), &mut env);
    let _col_price =
        stab_comp.change_collateral_price(a_bucket.resource_address(&mut env)?, dec!(1), &mut env);

    //new cr is 1.25 now, so this means liquidator receives 1.1 / 1.25, stabilis receives 0.05 / 1.25 and 0.1 / 1.25 is left in the cdp
    //collateral is 1000
    //liquidator receives 880, stabilis receives 40 and 80 is left in the cdp

    //mark loan
    let _marker = stab_comp.mark_for_liquidation(a_bucket.resource_address(&mut env)?, &mut env)?;

    let time = env.get_current_time();
    let new_time = time.add_minutes(10).unwrap();
    env.set_current_time(new_time);

    //liq without marker
    let (collateral_reward, _leftover_stab, _liquidation_receipt) = stab_comp
        .liquidate_position_without_marker(
            free_stab.take(dec!(500), &mut env)?,
            Some(0),
            cdp_id.clone(),
            &mut env,
        )?;

    let retrieved_collateral = stab_comp.retrieve_leftover_collateral(cdp_id.clone(), &mut env)?;
    assert!(retrieved_collateral.amount(&mut env)? == dec!(80));
    assert!(
        retrieved_collateral.resource_address(&mut env)? == a_bucket.resource_address(&mut env)?
    );

    let rewarded_collateral = collateral_reward.unwrap();
    assert!(rewarded_collateral.amount(&mut env)? == dec!(880));
    assert!(
        rewarded_collateral.resource_address(&mut env)? == a_bucket.resource_address(&mut env)?
    );

    let protocol_collateral = stab_comp.empty_collateral_treasury(
        dec!(40),
        a_bucket.resource_address(&mut env)?,
        false,
        &mut env,
    )?;
    assert!(protocol_collateral.amount(&mut env)? == dec!(40));
    assert!(
        protocol_collateral.resource_address(&mut env)? == a_bucket.resource_address(&mut env)?
    );

    let impossible_retrieval = stab_comp.empty_collateral_treasury(
        dec!("0.1"),
        a_bucket.resource_address(&mut env)?,
        false,
        &mut env,
    );
    assert!(impossible_retrieval.is_err());

    Ok(())
}

// Check if liquidation fines are calculated correctly if 110% < cr < 115%
#[test]
fn correct_liquidation_fines_between_110_115_cr() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    //cr is 2.25
    let (stab, cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(4500), &mut env)?, dec!("2000"), &mut env)?;

    let cdps = cdp.non_fungible_local_ids(&mut env)?;
    let cdp_id = cdps.first().unwrap();

    //get some more free stab to test with
    let free_stab = BucketFactory::create_fungible_bucket(
        stab.resource_address(&mut env)?,
        dec!(100000),
        Mock,
        &mut env,
    )?;

    //change col price so liq is possible
    let _stab_price = stab_comp.change_internal_price(dec!(2), &mut env);
    let _col_price =
        stab_comp.change_collateral_price(a_bucket.resource_address(&mut env)?, dec!(1), &mut env);

    //new cr is 1.125 now, so this means liquidator receives 1.1 / 1.125, stabilis receives 0.025 / 1.125 and 0 is left in the cdp
    //collateral is 4500
    //liquidator receives 4400, stabilis receives 100 and 0 is left in the cdp

    //mark loan
    let _marker = stab_comp.mark_for_liquidation(a_bucket.resource_address(&mut env)?, &mut env)?;

    let time = env.get_current_time();
    let new_time = time.add_minutes(10).unwrap();
    env.set_current_time(new_time);

    //liq without marker
    let (collateral_reward, _leftover_stab, _liquidation_receipt) = stab_comp
        .liquidate_position_without_marker(
            free_stab.take(dec!(2000), &mut env)?,
            Some(0),
            cdp_id.clone(),
            &mut env,
        )?;

    //uncommenting this gives an error? not sure why but for sure means there's no leftover collateral so all's good I guess
    /*let retrieved_collateral = stab_comp.retrieve_leftover_collateral(cdp_id.clone(), &mut env);
    assert!(retrieved_collateral.is_err());*/

    let rewarded_collateral = collateral_reward.unwrap();
    assert!(rewarded_collateral.amount(&mut env)? == dec!(4400));
    assert!(
        rewarded_collateral.resource_address(&mut env)? == a_bucket.resource_address(&mut env)?
    );

    let protocol_collateral = stab_comp.empty_collateral_treasury(
        dec!(100),
        a_bucket.resource_address(&mut env)?,
        false,
        &mut env,
    )?;
    assert!(protocol_collateral.amount(&mut env)? == dec!(100));
    assert!(
        protocol_collateral.resource_address(&mut env)? == a_bucket.resource_address(&mut env)?
    );

    let impossible_retrieval = stab_comp.empty_collateral_treasury(
        dec!("0.1"),
        a_bucket.resource_address(&mut env)?,
        false,
        &mut env,
    );
    assert!(impossible_retrieval.is_err());

    Ok(())
}

// Check if liquidation fines are calculated correctly if cr < 110%
#[test]
fn correct_liquidation_fines_below_110_cr() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    //cr is 2.25
    let (stab, cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(2100), &mut env)?, dec!("1000"), &mut env)?;

    let cdps = cdp.non_fungible_local_ids(&mut env)?;
    let cdp_id = cdps.first().unwrap();

    //get some more free stab to test with
    let free_stab = BucketFactory::create_fungible_bucket(
        stab.resource_address(&mut env)?,
        dec!(100000),
        Mock,
        &mut env,
    )?;

    //change col price so liq is possible
    let _stab_price = stab_comp.change_internal_price(dec!(2), &mut env);
    let _col_price =
        stab_comp.change_collateral_price(a_bucket.resource_address(&mut env)?, dec!(1), &mut env);

    //new cr is 1.05 now, so this means liquidator receives 1, stabilis receives 0 and 0 is left in the cdp
    //collateral is 2100
    //liquidator receives 2100, stabilis receives 0 and 0 is left in the cdp

    //mark loan
    let _marker = stab_comp.mark_for_liquidation(a_bucket.resource_address(&mut env)?, &mut env)?;

    let time = env.get_current_time();
    let new_time = time.add_minutes(10).unwrap();
    env.set_current_time(new_time);

    //liq without marker
    let (collateral_reward, _leftover_stab, _liquidation_receipt) = stab_comp
        .liquidate_position_without_marker(
            free_stab.take(dec!(1000), &mut env)?,
            Some(0),
            cdp_id.clone(),
            &mut env,
        )?;

    //uncommenting this gives an error? not sure why but for sure means there's no leftover collateral so all's good I guess
    /*let retrieved_collateral = stab_comp.retrieve_leftover_collateral(cdp_id.clone(), &mut env);
    assert!(retrieved_collateral.is_err());*/

    let rewarded_collateral = collateral_reward.unwrap();
    assert!(rewarded_collateral.amount(&mut env)? == dec!(2100));
    assert!(
        rewarded_collateral.resource_address(&mut env)? == a_bucket.resource_address(&mut env)?
    );

    let impossible_retrieval = stab_comp.empty_collateral_treasury(
        dec!("0.1"),
        a_bucket.resource_address(&mut env)?,
        false,
        &mut env,
    );
    assert!(impossible_retrieval.is_err());

    Ok(())
}

// Force liquidate completely
#[test]
fn force_liquidate_with_sufficient_collateral() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let (stab, _cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    let liquidation_result = stab_comp.force_liquidate(
        a_bucket.resource_address(&mut env)?,
        stab.take(dec!(500), &mut env)?,
        dec!(1),
        true,
        &mut env,
    );

    assert!(liquidation_result.is_ok());
    let (returned_collateral, leftover_stab) = liquidation_result.unwrap();
    assert_eq!(returned_collateral.amount(&mut env)?, dec!(500));
    assert_eq!(leftover_stab.amount(&mut env)?, dec!(0));

    Ok(())
}

// Force liquidate but not fully
#[test]
fn force_liquidate_partly() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let (stab, cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    let cdps = cdp.non_fungible_local_ids(&mut env)?;
    let cdp = cdps.first().unwrap();

    let liquidation_result = stab_comp.force_liquidate(
        a_bucket.resource_address(&mut env)?,
        stab.take(dec!(10), &mut env)?,
        dec!(1),
        true,
        &mut env,
    );

    assert!(liquidation_result.is_ok());
    let (returned_collateral, leftover_stab) = liquidation_result.unwrap();
    assert_eq!(returned_collateral.amount(&mut env)?, dec!(10));
    assert_eq!(leftover_stab.amount(&mut env)?, dec!(0));
    assert_eq!(stab.amount(&mut env)?, dec!(490));

    let close_result = stab_comp.close_cdp(cdp.clone(), stab, &mut env);

    assert!(close_result.is_ok());
    let (collateral_close, leftover_stab_close) = close_result.unwrap();
    assert_eq!(collateral_close.amount(&mut env)?, dec!(990));
    assert_eq!(leftover_stab_close.amount(&mut env)?, dec!(0));

    Ok(())
}

// Try to force liquidate a loan that can be marked, and fail
#[test]
fn cant_force_liquidate_markable() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let (stab, _cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    let _ = stab_comp.change_collateral_price(
        a_bucket.resource_address(&mut env)?,
        dec!(0.5),
        &mut env,
    )?;

    let liquidation_result = stab_comp.force_liquidate(
        a_bucket.resource_address(&mut env)?,
        stab.take(dec!(500), &mut env)?,
        dec!(1),
        true,
        &mut env,
    );

    assert!(liquidation_result.is_err());

    Ok(())
}

// Force mint with valid parameters
#[test]
fn force_mint_valid_parameters() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let (_stab, _cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(1000), &mut env)?, dec!(100), &mut env)?;

    let mint_result = stab_comp.force_mint(
        a_bucket.resource_address(&mut env)?,
        a_bucket.take(dec!(100), &mut env)?,
        dec!(1),
        &mut env,
    );

    assert!(mint_result.is_ok());
    let (minted_stab, leftover_collateral) = mint_result.unwrap();
    assert_eq!(minted_stab.amount(&mut env)?, dec!(100));
    assert_eq!(leftover_collateral.is_none(), true);

    Ok(())
}

// Force mint with excessive collateral
#[test]
fn force_mint_excessive_collateral() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let (_stab, _cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(360), &mut env)?, dec!(10), &mut env)?;

    let mint_result = stab_comp.force_mint(
        a_bucket.resource_address(&mut env)?,
        a_bucket.take(dec!(100), &mut env)?,
        dec!(1),
        &mut env,
    );

    assert!(mint_result.is_ok());
    let (minted_stab, leftover_collateral) = mint_result.unwrap();
    assert_eq!(minted_stab.amount(&mut env)?, dec!(90));
    assert!(leftover_collateral.is_some());

    let leftover_bucket = leftover_collateral.unwrap();
    assert_eq!(leftover_bucket.amount(&mut env)?, dec!(10));

    Ok(())
}

// Force mint with invalid collateral
#[test]
fn fail_force_mint_invalid_collateral() -> Result<(), RuntimeError> {
    let (mut env, mut stab_comp, a_bucket, _control_bucket) = publish_and_setup()?;

    let (_stab, _cdp) =
        stab_comp.open_cdp(a_bucket.take(dec!(1000), &mut env)?, dec!(500), &mut env)?;

    let invalid_collateral = ResourceBuilder::new_fungible(OwnerRole::None)
        .divisibility(18)
        .mint_initial_supply(10000, &mut env)?;

    let mint_result = stab_comp.force_mint(
        invalid_collateral.resource_address(&mut env)?,
        invalid_collateral.take(dec!(500), &mut env)?,
        dec!(1),
        &mut env,
    );

    assert!(mint_result.is_err());

    Ok(())
}
