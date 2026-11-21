from dataclasses import dataclass
from datetime import datetime
from decimal import Decimal


@dataclass(frozen=True)
class Line:
    sku: str
    qty: int
    unit_price: Decimal
    amount: Decimal


@dataclass(frozen=True)
class Order:
    id: int | None
    customer: str
    reservation_id: str
    status: str
    total: Decimal
    notes: str
    created_at: datetime
    lines: tuple[Line, ...]
