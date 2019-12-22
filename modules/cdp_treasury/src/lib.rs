#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{decl_error, decl_module, decl_storage, traits::Get, Parameter};
use orml_traits::{
	arithmetic::{self, Signed},
	MultiCurrency, MultiCurrencyExtended,
};
use rstd::{
	convert::{TryFrom, TryInto},
	result,
};
use sp_runtime::{
	traits::{
		AccountIdConversion, CheckedAdd, Convert, MaybeSerializeDeserialize, Member, Saturating, SimpleArithmetic, Zero,
	},
	ModuleId,
};
use support::CDPTreasury;

mod mock;
mod tests;

const MODULE_ID: ModuleId = ModuleId(*b"aca/trsy");

type BalanceOf<T> = <<T as Trait>::Currency as MultiCurrency<<T as system::Trait>::AccountId>>::Balance;

pub trait Trait: system::Trait {
	type Currency: MultiCurrencyExtended<Self::AccountId, CurrencyId = Self::CurrencyId>;
	type GetStableCurrencyId: Get<Self::CurrencyId>;
	type CurrencyId: Parameter + Member + Copy + MaybeSerializeDeserialize;
	type DebitBalance: Parameter + Member + SimpleArithmetic + Default + Copy + MaybeSerializeDeserialize;
	type DebitAmount: Signed
		+ TryInto<Self::DebitBalance>
		+ TryFrom<Self::DebitBalance>
		+ Parameter
		+ Member
		+ arithmetic::SimpleArithmetic
		+ Default
		+ Copy
		+ MaybeSerializeDeserialize;
	type Convert: Convert<(Self::CurrencyId, Self::DebitBalance), BalanceOf<Self>>;
}

decl_error! {
	/// Error for cdp treasury module.
	pub enum Error {
		DebitDepositFailed,
		DebitWithdrawFailed,
		AmountIntoDebitBalanceFailed,
	}
}

decl_storage! {
	trait Store for Module<T: Trait> as CDPTreasury {
		DebitPool get(fn debit_pool): BalanceOf<T>;
		SurplusPool get(fn surplus_pool): BalanceOf<T>;
	}
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		fn on_finalize(_now: T::BlockNumber) {
			let amount = rstd::cmp::min(Self::debit_pool(), Self::surplus_pool());
			if !amount.is_zero() {
				if T::Currency::withdraw(T::GetStableCurrencyId::get(), &Self::account_id(), amount).is_ok() {
					<DebitPool<T>>::mutate(|debit| *debit -= amount);
					<SurplusPool<T>>::mutate(|surplus| *surplus -= amount);
				}
			}
		}
	}
}

impl<T: Trait> Module<T> {
	pub fn account_id() -> T::AccountId {
		MODULE_ID.into_account()
	}
}

impl<T: Trait> CDPTreasury for Module<T> {
	type Balance = BalanceOf<T>;

	fn on_debit(amount: Self::Balance) {
		<DebitPool<T>>::mutate(|debit| *debit = debit.saturating_add(amount));
	}

	fn on_surplus(amount: Self::Balance) {
		if T::Currency::balance(T::GetStableCurrencyId::get(), &Self::account_id())
			.checked_add(&amount)
			.is_some()
		{
			T::Currency::deposit(T::GetStableCurrencyId::get(), &Self::account_id(), amount)
				.expect("never failed after overflow check");
			<SurplusPool<T>>::mutate(|surplus| *surplus += amount);
		}
	}
}

impl<T: Trait> MultiCurrency<T::AccountId> for Module<T> {
	type Balance = T::DebitBalance;
	type CurrencyId = T::CurrencyId;
	type Error = Error;

	// be of no effect
	fn ensure_can_withdraw(
		_currency_id: Self::CurrencyId,
		_who: &T::AccountId,
		_amount: Self::Balance,
	) -> result::Result<(), Self::Error> {
		Ok(())
	}

	// be of no effect
	fn total_issuance(_currency_id: Self::CurrencyId) -> Self::Balance {
		Self::Balance::default()
	}

	// be of no effect
	fn balance(_currency_id: Self::CurrencyId, _who: &T::AccountId) -> Self::Balance {
		Self::Balance::default()
	}

	// be of no effect
	fn transfer(
		_currency_id: Self::CurrencyId,
		_from: &T::AccountId,
		_to: &T::AccountId,
		_amount: Self::Balance,
	) -> result::Result<(), Self::Error> {
		Ok(())
	}

	fn deposit(
		currency_id: Self::CurrencyId,
		who: &T::AccountId,
		debit_amount: Self::Balance,
	) -> result::Result<(), Self::Error> {
		let stable_coin_amount: BalanceOf<T> = T::Convert::convert((currency_id, debit_amount));
		T::Currency::deposit(T::GetStableCurrencyId::get(), who, stable_coin_amount)
			.map_err(|_| Error::DebitDepositFailed)
	}

	fn withdraw(
		currency_id: Self::CurrencyId,
		who: &T::AccountId,
		debit_amount: Self::Balance,
	) -> result::Result<(), Self::Error> {
		let stable_coin_amount: BalanceOf<T> = T::Convert::convert((currency_id, debit_amount));
		T::Currency::withdraw(T::GetStableCurrencyId::get(), who, stable_coin_amount)
			.map_err(|_| Error::DebitWithdrawFailed)
	}

	// be of no effect
	fn slash(_currency_id: Self::CurrencyId, _who: &T::AccountId, _amount: Self::Balance) -> Self::Balance {
		Self::Balance::default()
	}
}

impl<T: Trait> MultiCurrencyExtended<T::AccountId> for Module<T> {
	type Amount = T::DebitAmount;

	fn update_balance(
		currency_id: Self::CurrencyId,
		who: &T::AccountId,
		debit_amount: Self::Amount,
	) -> Result<(), Self::Error> {
		let debit_balance =
			TryInto::<Self::Balance>::try_into(debit_amount.abs()).map_err(|_| Error::AmountIntoDebitBalanceFailed)?;
		if debit_amount.is_positive() {
			Self::deposit(currency_id, who, debit_balance)
		} else {
			Self::withdraw(currency_id, who, debit_balance)
		}
	}
}
