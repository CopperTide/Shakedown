"""plant-opcua: Ridgeline Plant 2's floor as an OPC UA server, with a bridge that browses, reads and writes tags."""
from shakedown_part import serve

from .part import PlantOpcua

serve(PlantOpcua())
