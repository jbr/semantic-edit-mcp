import logging

log = logging.getLogger(__name__)


def summarize(results):
    passed = 0
    failed = []
    for result in results:
        if result.ok:
            passed += 1
        else:
            failed.append(result.name)
    log.info("%d passed, %d failed", passed, len(failed))
    return passed, failed
