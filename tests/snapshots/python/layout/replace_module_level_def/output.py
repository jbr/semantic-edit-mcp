def usage(prog):
    name = prog.rsplit("/", 1)[-1]
    return (
        f"usage: {name} [-h] [-v] FILE\n"
        "  -h, --help     show this help message and exit\n"
        "  -v, --verbose  print each file as it is processed"
    )


def main(argv):
    print(usage(argv[0]))
