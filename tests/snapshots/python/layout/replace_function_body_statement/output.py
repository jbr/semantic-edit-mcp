def parse_failure(text):
    key, _, rest = text.partition(":")
    return key.strip(), rest.strip(), text


def check(result):
    if result.ok:
        return True, None, None
    key, line, _full = parse_failure(result.err or result.out)
    return False, key, line
