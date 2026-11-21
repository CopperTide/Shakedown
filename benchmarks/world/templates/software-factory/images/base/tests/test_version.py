from platform_lib.version import image_sha, version_doc


def test_the_sha_is_the_image_tag():
    assert image_sha({"APP_IMAGE": "registry.test:5000/shop/orders:4f2a9c1"}) == "4f2a9c1"
    assert image_sha({"APP_SHA": "abc", "APP_IMAGE": "orders:def"}) == "abc"
    assert image_sha({}) == "dev"


def test_deps_are_the_installed_versions_of_the_pinned_requirements(tmp_path):
    req = tmp_path / "requirements.txt"
    req.write_text("# what the service needs\nhttpx==0.0.1\npytest>=1  # not pinned: left out\n\n")
    doc = version_doc(req, {"APP_IMAGE": "orders:abc"})
    import httpx
    major = int(httpx.__version__.split(".")[0])
    assert doc == {"sha": "abc", "deps": {"httpx": httpx.__version__, "httpx_major": major}}
