def drain(queue, sink, transform):
    written = 0
    while queue:
        item = queue.pop()
        if item is None:
            break
        if item.skip:
            continue
        item = transform(item)
        sink.write(item)
        written += 1
        if written % 100 == 0:
            sink.flush()
    sink.flush()
    return written
