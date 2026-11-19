"""What every resolver of one GraphQL request works with: the caller, the public base, and whether a
mutation wrote anything (the answer's `X-Arena-Effect`)."""
from ..view import View


class Gql(View):
    def __init__(self, ctx, base: str, user, readonly: bool = False):
        super().__init__(ctx, base)
        self.user, self.readonly, self.wrote = user, readonly, False

    @property
    def login(self) -> str:
        return self.user["login"]
