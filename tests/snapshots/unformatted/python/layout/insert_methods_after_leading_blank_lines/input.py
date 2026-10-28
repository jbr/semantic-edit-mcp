class Basket:
    def __init__(self):
        self.items = []

    def add(self, item):
        self.items.append(item)


def make_basket():
    return Basket()
