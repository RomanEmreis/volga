#!/bin/bash
# tests/tls/generate.sh

set -e

echo "Generating Test TLS Certificates for mTLS..."

# 1. Create CA (Certificate Authority)
echo "1. Creating CA..."
openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout ca.key \
  -out ca.pem \
  -days 3650 \
  -subj "/CN=Test CA"

# 2. Create server certificate
echo "2. Creating server certificate..."

cat > server.conf << EOF
[req]
distinguished_name = req_distinguished_name
req_extensions = v3_req
prompt = no

[req_distinguished_name]
CN = localhost

[v3_req]
keyUsage = keyEncipherment, dataEncipherment
extendedKeyUsage = serverAuth
subjectAltName = @alt_names

[alt_names]
DNS.1 = localhost
IP.1 = 127.0.0.1
EOF

openssl genrsa -out server.key 2048
openssl req -new -key server.key -out server.csr -config server.conf
openssl x509 -req -in server.csr \
  -CA ca.pem -CAkey ca.key \
  -CAcreateserial \
  -out server.pem \
  -days 3650 \
  -extensions v3_req \
  -extfile server.conf

# 3. Create client certificate
echo "3. Creating client certificate..."

cat > client.conf << EOF
[req]
distinguished_name = req_distinguished_name
req_extensions = v3_req
prompt = no

[req_distinguished_name]
CN = Test Client

[v3_req]
keyUsage = digitalSignature, keyEncipherment
extendedKeyUsage = clientAuth
EOF

openssl genrsa -out client.key 2048
openssl req -new -key client.key -out client.csr -config client.conf
openssl x509 -req -in client.csr \
  -CA ca.pem -CAkey ca.key \
  -CAcreateserial \
  -out client.pem \
  -days 3650 \
  -extensions v3_req \
  -extfile client.conf

# Clean temp files
rm -f server.csr client.csr server.conf client.conf ca.srl

echo "Done! Generated certificates:"
echo "  - ca.pem (CA certificate)"
echo "  - server.pem + server.key (Server certificate)"
echo "  - client.pem + client.key (Client certificate)"

# Check
echo ""
echo "Verification:"
openssl verify -CAfile ca.pem server.pem
openssl verify -CAfile ca.pem client.pem