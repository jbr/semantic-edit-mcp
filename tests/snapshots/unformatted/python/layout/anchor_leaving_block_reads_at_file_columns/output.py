class Greeter:
    def greet(self, name):
        return f"Hello, {name}!"


def main():
    greeter = Greeter()
    print(greeter.greet("world"))
