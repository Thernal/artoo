// GitLance's old address (`wrangler.toml` here): GitLance 1.3.0 sends its reports and asks what
// became of them here. Each request goes on to Artoo's relay as it came, its headers with it (the
// relay counts by `CF-Connecting-IP`), and its answer comes back unchanged.
export default {
  fetch(request, env) {
    return env.RELAY.fetch(request);
  },
};
