curl 'https://api.example.test/graph/v1/orders?expand=items&limit=20' \
  -H 'accept: application/json, text/plain, */*' \
  -H 'accept-language: en-US,en;q=0.9' \
  -H 'authorization: Bearer eyJhbGciOiJIUzI1NiJ9.demo.sig' \
  -H 'content-type: application/json' \
  -H 'origin: https://app.example.test' \
  -H 'user-agent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36' \
  --data-raw '{"status":"open","note":"it'\''s urgent"}'
