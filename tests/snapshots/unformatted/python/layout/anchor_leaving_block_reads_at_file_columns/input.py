class Greeter:
    def greet(self, name):
        return f"Hello, {name}"


def main():
    print(Greeter().greet("world"))
