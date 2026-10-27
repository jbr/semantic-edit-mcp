def load(path):
    with open(path) as f:
        data = f.read( )
        return {'data':data}


def main():
    print(load("data.txt"))
