// Simple HTTP handler component for testing
addEventListener('fetch', async (event) => {
  const url = new URL(event.request.url);
  const path = url.pathname;
  const method = event.request.method;

  let response;

  if (path === '/' && method === 'GET') {
    response = new Response(JSON.stringify({
      message: "Hello from HTTP handler!",
      status: "ok"
    }), {
      status: 200,
      headers: { "content-type": "application/json" }
    });
  } else if (path === '/echo' && method === 'GET') {
    response = new Response(JSON.stringify({
      path: path,
      method: method
    }), {
      status: 200,
      headers: { "content-type": "application/json" }
    });
  } else {
    response = new Response(JSON.stringify({
      error: "Not Found"
    }), {
      status: 404,
      headers: { "content-type": "application/json" }
    });
  }

  event.respondWith(response);
});
