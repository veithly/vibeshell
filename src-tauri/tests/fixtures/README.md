# Synthetic credential fixtures

`server-add-test-key` is an encrypted Ed25519 private key generated solely for
unit tests. Its passphrase is ` phrase ` (one space at each end). It is not used
by a real server. The tests verify exact key/passphrase preservation and parsing;
they do not read the user's key files, database or servers.
