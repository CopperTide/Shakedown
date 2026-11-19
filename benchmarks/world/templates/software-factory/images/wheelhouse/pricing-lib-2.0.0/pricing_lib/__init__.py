"""Money rounding and discounts shared by the shop's services. Amounts are rounded half up to the minor unit of
their currency (cents for USD and EUR, none for JPY)."""
from decimal import ROUND_HALF_UP, Decimal

__version__ = "2.0.0"
MINOR_UNITS = {"USD": 2, "EUR": 2, "GBP": 2, "CHF": 2, "CAD": 2, "AUD": 2, "SEK": 2, "JPY": 0, "KRW": 0, "KWD": 3}


def round_price(amount, currency: str) -> Decimal:
    """`amount` rounded half up to the minor unit of `currency` (an ISO 4217 code)."""
    if currency not in MINOR_UNITS:
        raise ValueError(f"unknown currency {currency}")
    return Decimal(str(amount)).quantize(Decimal(1).scaleb(-MINOR_UNITS[currency]), rounding=ROUND_HALF_UP)


def apply_discount(amount, percent, currency: str) -> Decimal:
    """`amount` less `percent` per cent, rounded to the minor unit of `currency`."""
    return round_price(Decimal(str(amount)) * (100 - Decimal(str(percent))) / 100, currency)
