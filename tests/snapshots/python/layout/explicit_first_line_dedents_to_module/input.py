def classify(items):
    counts = {"small": 0, "large": 0}
    for item in items:
        if item < 10:
            counts["small"] += 1
        elif item < 100:
            counts["large"] += 1
    return counts


def main():
    print(classify([1, 50, 500]))
