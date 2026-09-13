def factory():
    class Inner:
        pass

    return Inner


def target():
    return factory()
