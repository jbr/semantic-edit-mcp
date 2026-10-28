def load(path):
    with open(path) as f:
        return f.read()


def main():
    print(load("data.txt"))
