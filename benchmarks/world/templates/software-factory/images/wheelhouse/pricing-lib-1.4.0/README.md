# pricing-lib

Money rounding and discounts shared by the shop's services.

```python
from pricing_lib import apply_discount, round_price

round_price(Decimal("19.995"))          # Decimal("20.00")
apply_discount(Decimal("80.00"), 15)    # Decimal("68.00")
```

## Changes

### 1.4.0
- `apply_discount` accepts a float or string percent.

### 1.3.0
- `round_price` accepts floats (converted through `str`, so 12.344 rounds to 12.34).
