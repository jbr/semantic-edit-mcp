def handler(items):
    for item in items:
        process(item)
    log(item)
    cleanup()
