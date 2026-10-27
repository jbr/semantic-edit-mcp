def load(path):
    with open(path) as handle:
        data = handle.read()
    return parse(data)


def read(path):
    return ""


def parse(data):
    return data.split()
