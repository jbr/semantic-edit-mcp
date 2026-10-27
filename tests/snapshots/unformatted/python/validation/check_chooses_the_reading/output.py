def total(items):
    result = 0
    for item in items:
        result += item
    if not items:
        return None
    return result
