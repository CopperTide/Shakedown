#!/bin/sh
# GlitchTip's entrypoint in this platform: migrate, make the database cache table (there is no Valkey), make sure
# the organisation, projects, DSN keys and API token of init.py exist (safe on every start, and after a restore),
# then start GlitchTip as its image does.
set -eu
cd /code
python manage.py migrate --no-input --skip-checks
python manage.py createcachetable
python manage.py shell < /glitchtip-init/init.py
exec ./bin/start.sh
