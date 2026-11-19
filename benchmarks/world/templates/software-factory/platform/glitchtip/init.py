# Run by init.sh with `manage.py shell`. Makes, when missing: the admin user (SHOP_ADMIN_EMAIL), the organisation
# (SHOP_ORG) and its team, one project per `name=public_key` of SHOP_PROJECTS (created in that order, so their ids
# are 1, 2, 3 in a new database, as the apps' DSNs say), and the API token SHOP_API_TOKEN the platform's probes use.
import os
import uuid

from allauth.account.models import EmailAddress

from apps.api_tokens.models import APIToken
from apps.organizations_ext.models import Organization
from apps.projects.models import Project, ProjectKey
from apps.teams.models import Team
from apps.users.models import User

SCOPES = ["project:read", "project:write", "team:read", "event:read", "event:write", "org:read", "member:read"]


def admin():
    email = os.environ["SHOP_ADMIN_EMAIL"]
    user = User.objects.filter(email=email).first()
    if user is None:
        user = User.objects.create_superuser(email=email, password=os.environ["SHOP_ADMIN_PASSWORD"])
    EmailAddress.objects.get_or_create(user=user, email=email, defaults={"primary": True, "verified": True})
    return user


def organization(user):
    slug = os.environ["SHOP_ORG"]
    org, _ = Organization.objects.get_or_create(slug=slug, defaults={"name": slug})
    if not org.users.filter(pk=user.pk).exists():
        org.add_user(user)
    return org


def project(org, team, number: int, slug: str, public_key: str):
    found, _ = Project.objects.get_or_create(slug=slug, organization=org, defaults={"name": slug})
    if found.id != number:
        raise SystemExit(f"project {slug} has id {found.id}; the apps' DSNs expect {number}")
    team.projects.add(found)
    key = uuid.UUID(public_key)
    if not ProjectKey.objects.filter(project=found, public_key=key).exists():
        first = ProjectKey.objects.filter(project=found).first()
        if first is None:
            ProjectKey.objects.create(project=found, public_key=key)
        else:
            ProjectKey.objects.filter(pk=first.pk).update(public_key=key)


def token(user):
    value = os.environ["SHOP_API_TOKEN"]
    if not APIToken.objects.filter(token=value).exists():
        made = APIToken.objects.create(user=user, label="platform probes")
        APIToken.objects.filter(pk=made.pk).update(token=value)
        made.refresh_from_db()
        made.add_permissions(SCOPES)


def main():
    user = admin()
    org = organization(user)
    team, _ = Team.objects.get_or_create(slug="engineering", organization=org)
    team.members.add(org.organization_users.get(user=user))
    for number, pair in enumerate(os.environ["SHOP_PROJECTS"].split(","), 1):
        slug, public_key = pair.split("=")
        project(org, team, number, slug, public_key)
    token(user)
    print("glitchtip ready:", org.slug, [p.slug for p in Project.objects.filter(organization=org).order_by("id")])


main()
