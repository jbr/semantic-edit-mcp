import functools


class Report:
    def __init__(self, rows):
        self.rows = rows

    @property
    @functools.cache
    def total(self):
        return sum(self.rows)
