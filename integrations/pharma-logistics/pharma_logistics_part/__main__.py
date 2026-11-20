"""pharma-logistics: simulated cold-chain carriers, customer notices and a supply chain director."""
from shakedown_part import serve

from .part import PharmaLogistics

serve(PharmaLogistics())
