curl -X POST http://localhost:8080/cal \
  -H "Content-Type: application/json" \
  -d '{"text": "Team meeting tomorrow at 3pm for one hour", "tz": "America/New_York"}'
