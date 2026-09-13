def factory():
    class Inner:
        pass

    return Inner


def target():
    return factory()


def inserted():
    return 1
