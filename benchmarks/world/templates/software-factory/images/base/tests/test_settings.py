from platform_lib.settings import Settings


def test_settings_come_from_the_environment():
    s = Settings.from_env("orders", {"DEPLOY_ENV": "production", "DATABASE_URL": "postgresql://db/shop",
                                     "APP_IMAGE": "shop/orders:abc", "DB_POOL_SIZE": "3"})
    assert (s.environment, s.database_url, s.db_schema, s.sha, s.pool_size) == (
        "production", "postgresql://db/shop", "orders", "abc", 3)
    assert Settings.from_env("orders", {"DB_SCHEMA": "sales"}).db_schema == "sales"
