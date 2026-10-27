class Cache:
    def __init__(self):
        self.data = {}
        self.hits = 0
        self.misses = 0

    def get(self, key):
        if key in self.data:
            return self.data[key]
        return None
