class Cache:
    def __init__(self):
        self.data = {}
        self.hits = 0
        self.misses = 0

    def get(self, key):
        if key in self.data:
            self.hits += 1
            return self.data[key]
        self.misses += 1
        return None
