# Offline log-integrity fixtures

The two runtime-log.txt files are byte-for-byte copies of the r3 S4 Pandemonium
A1 and B2 logs. R3-SOURCES.json records their source paths and SHA-256 hashes.
They are regression fixtures only and cannot qualify any r4 console run.
The original r3 evidence is never modified by setup or tests.

The four original cases remain: complete, nested tag, orphan field and
different same-arm config length. The negative-2 files have a 42-byte A
record and a 40-byte B record: false versus true plus one space after the A colon.
The JSON remains valid, but their A-minus-B length difference is 2 bytes.
The fixture runner uses temporary sessions and the package protocol, checks
both historical logs in A1/B2 order, and additionally covers ABBA repetition,
same-arm mismatches, other cross-arm differences and session/order isolation.

Run scripts/test-log-integrity.py directly or through test-console-tools.py.
All gate subprocesses run only the offline log-integrity helper.
