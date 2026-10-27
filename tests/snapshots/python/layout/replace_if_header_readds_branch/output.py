def resolve(p, owner):
    if p == "cls":
        if owner is None:
            return None
        return type(owner)
    if p == "self":
        return owner
    if p == "super":
        return owner.__class__.__mro__[1]
    return None
