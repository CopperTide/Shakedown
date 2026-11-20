"""line-sim: a discrete-event simulation study of a plant (shakedown-part/1)."""
from shakedown_part import serve

from .part import LineSim

serve(LineSim())
