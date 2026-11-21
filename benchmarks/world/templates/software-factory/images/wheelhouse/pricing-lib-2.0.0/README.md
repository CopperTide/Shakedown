# pricing-lib

Money rounding and discounts shared by the shop's services.

```python
from pricing_lib import apply_discount, round_price

round_price(Decimal("19.995"), "USD")        # Decimal("20.00")
round_price(Decimal("1999.5"), "JPY")        # Decimal("2000")
apply_discount(Decimal("80.00"), 15, "EUR")  # Decimal("68.00")
```

## Changes

### 2.0.0 (breaking)
- `round_price(amount, currency)` and `apply_discount(amount, percent, currency)` take the ISO 4217 currency
  and round to its minor unit (none for JPY and KRW, three places for KWD). Calls without a currency raise
  `TypeError`; an unknown currency raises `ValueError`. To upgrade, pass the currency of the amount at every
  call site.

### 1.4.0
- `apply_discount` accepts a float or string percent.

### 1.3.0
- `round_price` accepts floats (converted through `str`, so 12.344 rounds to 12.34).
