def resolve(p, owner):
    if p == "self":
        return owner
    if p == "super":
        return owner.__class__.__mro__[1]
    return None
