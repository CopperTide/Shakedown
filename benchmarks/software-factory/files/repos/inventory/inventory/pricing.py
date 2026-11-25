"""What a product sells for: its list price, less its promotion when it has one."""
from decimal import Decimal

from pricing_lib import round_price

from .models import Product


def unit_price(product: Product) -> Decimal:
    if not product.discount_pct:
        return product.list_price
    return round_price(product.list_price * (100 - product.discount_pct) / 100)
