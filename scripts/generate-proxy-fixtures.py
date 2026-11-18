"""Generate ONLY synthetic, untrusted localhost TLS fixtures for proxy tests.

Requires openssl. The CA signing key is temporary and is never retained.
Never import this CA into a system/browser trust store or use these keys outside
tests. Rust adds its certificate only under cfg(test).
"""
from pathlib import Path
import subprocess
import tempfile


def main() -> None:
    destination = Path(__file__).resolve().parents[1] / "src-tauri/tests/fixtures"
    with tempfile.TemporaryDirectory(prefix="vibeshell-proxy-test-") as directory:
        root = Path(directory)
        (root / "ca.cnf").write_text("""[req]
prompt=no
distinguished_name=dn
x509_extensions=ca
[dn]
CN=VibeShell TEST ONLY Proxy CA
[ca]
basicConstraints=critical,CA:true
keyUsage=critical,keyCertSign,cRLSign
subjectKeyIdentifier=hash
""", encoding="utf-8")
        (root / "leaf.cnf").write_text("""[req]
prompt=no
distinguished_name=dn
[dn]
CN=localhost
[leaf]
basicConstraints=critical,CA:false
keyUsage=critical,digitalSignature,keyEncipherment
extendedKeyUsage=serverAuth
subjectAltName=DNS:localhost
""", encoding="utf-8")

        def run(*args: str) -> None:
            subprocess.run(["openssl", *args], cwd=root, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)

        run("req", "-x509", "-newkey", "rsa:2048", "-nodes", "-sha256", "-days", "3650", "-config", "ca.cnf", "-keyout", "ca.key", "-out", "ca.pem")
        run("req", "-new", "-newkey", "rsa:2048", "-nodes", "-sha256", "-config", "leaf.cnf", "-keyout", "leaf.key", "-out", "leaf.csr")
        run("x509", "-req", "-in", "leaf.csr", "-CA", "ca.pem", "-CAkey", "ca.key", "-CAcreateserial", "-days", "3650", "-sha256", "-extfile", "leaf.cnf", "-extensions", "leaf", "-out", "leaf.pem")
        run("x509", "-in", "ca.pem", "-outform", "DER", "-out", str(destination / "proxy-test-ca.der"))
        run("x509", "-in", "leaf.pem", "-outform", "DER", "-out", str(destination / "proxy-test-leaf.der"))
        run("pkcs8", "-topk8", "-nocrypt", "-in", "leaf.key", "-outform", "DER", "-out", str(destination / "proxy-test-key.der"))
    print("Created three untrusted localhost TLS test fixtures; no CA signing key retained.")


if __name__ == "__main__":
    main()
