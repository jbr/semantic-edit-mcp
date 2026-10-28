import logging

log = logging.getLogger(__name__)


def summarize(results):
    passed = 0
    failed = []
    for result in results:
        if result.ok:
            passed += 1
        elif result.skipped:
            continue
        else:
            failed.append(result.name)
            log.warning(
                "test %s failed after %.2fs with %d assertion errors; "
                "rerun it with --verbose to see the captured output",
                result.name,
                result.duration,
                len(result.errors),
            )
    log.info("%d passed, %d failed", passed, len(failed))
    return passed, failed
