from dataclasses import dataclass
from decimal import Decimal


@dataclass(frozen=True)
class Product:
    sku: str
    name: str
    list_price: Decimal
    discount_pct: int
    currency: str
    on_hand: int


@dataclass(frozen=True)
class Line:
    sku: str
    qty: int
    unit_price: Decimal

    @property
    def amount(self) -> Decimal:
        return self.unit_price * self.qty


@dataclass(frozen=True)
class Reservation:
    id: str
    lines: tuple[Line, ...]
    expires_at: int  # epoch seconds
    status: str  # held | committed

    @property
    def total(self) -> Decimal:
        return sum((line.amount for line in self.lines), Decimal("0"))
