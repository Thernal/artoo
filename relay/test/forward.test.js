import { test } from 'node:test';
import assert from 'node:assert/strict';
import forward from '../forward/worker.js';

test("GitLance's address hands the request to Artoo's relay as it came", async () => {
  let seen;
  const env = {
    RELAY: {
      async fetch(request) {
        seen = request;
        return new Response('{"id":"R-7F3A2C"}', { status: 200 });
      },
    },
  };
  const request = new Request('https://gitlance-reports.example/v1/report', {
    method: 'POST',
    headers: { 'CF-Connecting-IP': '203.0.113.7' },
    body: '{}',
  });
  const answer = await forward.fetch(request, env);
  assert.equal(seen, request);
  assert.equal(answer.status, 200);
  assert.equal(await answer.text(), '{"id":"R-7F3A2C"}');
});
