curl -sS http://localhost:8080/v1/chat/completions \
    -H "Authorization: Bearer lp_019df123-d579-7043-8ce0-f7ebebad8c85_019df123-d579-7043-8ce0-f7fdc7f70e3e" \
    -H "Content-Type: application/json" \
    -d '{
      "model": "Qwen/Qwen3.6-27B-FP8",
      "messages": [
        { "role": "user", "content": "Write a haiku about SQLite." }
      ],
      "stream": true
    }'
