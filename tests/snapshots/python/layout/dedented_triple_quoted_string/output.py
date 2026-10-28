class Report:
    def __init__(self, lines):
        self.lines = lines

    def render(self):
        header = """\
Report
======
  generated from {count} lines
"""
        header = header.format(count=len(self.lines))
        return header
