def parse_failure(text):
    key, _, rest = text.partition(":")
    return key.strip(), rest.strip(), text


def check(result):
    if result.ok:
        return True, None, None
    return (
        (False, *parse_failure(result.err)[:2])
        if result.err
        else (lambda key, line, _full: (False, key, line))(*parse_failure(result.out))
    )
