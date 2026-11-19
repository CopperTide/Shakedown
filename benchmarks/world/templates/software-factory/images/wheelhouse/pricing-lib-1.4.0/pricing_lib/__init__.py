"""Money rounding and discounts shared by the shop's services. Amounts are rounded half up to cents."""
from decimal import ROUND_HALF_UP, Decimal

__version__ = "1.4.0"
CENT = Decimal("0.01")


def round_price(amount) -> Decimal:
    """`amount` rounded half up to cents."""
    return Decimal(str(amount)).quantize(CENT, rounding=ROUND_HALF_UP)


def apply_discount(amount, percent) -> Decimal:
    """`amount` less `percent` per cent, rounded to cents."""
    return round_price(Decimal(str(amount)) * (100 - Decimal(str(percent))) / 100)
