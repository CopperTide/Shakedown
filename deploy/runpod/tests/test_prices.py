from conftest import T0
from gpupod.prices import current_rate, quote_line


def test_quote_reads_the_clouds_price_and_null_is_not_offered(runner):
    q = current_rate(runner, "NVIDIA H100 80GB HBM3", "COMMUNITY", now=lambda: T0)
    assert (q.gpu_id, q.cloud, q.usd_h, q.available) == ("NVIDIA H100 80GB HBM3", "COMMUNITY", 2.69, True)
    assert (q.source, q.read_at) == ("runpodctl gpu list", "2026-11-19T21:29Z")
    assert current_rate(runner, "NVIDIA H100 80GB HBM3", "SECURE", now=lambda: T0).usd_h == 4.39
    b200 = current_rate(runner, "NVIDIA B200", "COMMUNITY", now=lambda: T0)
    assert b200.usd_h is None and b200.available is False
    nothing = current_rate(runner, "NVIDIA RTX 9999", "COMMUNITY", now=lambda: T0)
    assert nothing.usd_h is None and nothing.available is False
    assert runner.calls == [["gpu", "list", "--include-unavailable"]] * 4


def test_a_gpu_without_stock_is_not_available(runner):
    runner.set_gpu("NVIDIA H100 80GB HBM3", available=False, stockStatus=None)
    q = current_rate(runner, "NVIDIA H100 80GB HBM3", "COMMUNITY", now=lambda: T0)
    assert q.usd_h == 2.69 and q.available is False


def test_the_quote_line(runner):
    q = current_rate(runner, "NVIDIA H100 80GB HBM3", "COMMUNITY", now=lambda: T0)
    assert quote_line(q, spent=0.0, budget=50.0, hours=8) == (
        "NVIDIA H100 80GB HBM3, community: $2.69/h (runpodctl gpu list, 2026-11-19T21:29Z). "
        "Spent so far $0.00 of $50.00; 8 h at this rate would bring it to $21.52.")
