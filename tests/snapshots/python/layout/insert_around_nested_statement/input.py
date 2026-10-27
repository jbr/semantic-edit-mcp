def drain(queue, sink, transform):
    written = 0
    while queue:
        item = queue.pop()
        if item is None:
            break
        sink.write(item)
    sink.flush()
    return written
