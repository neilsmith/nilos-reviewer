Bumps [acme-auth](https://example.invalid/acme/acme-auth) from 3.1.0 to 3.2.0.
<details>
<summary>Changelog</summary>
<p><em>Sourced from <a href="https://example.invalid/acme/acme-auth/blob/main/CHANGELOG.md">acme-auth's changelog</a>.</em></p>
<blockquote>
<h2>3.2.0</h2>
<ul>
<li><code>persist_login</code> no longer raises <code>KeyError</code> when called outside the request phase, before <code>acme_auth.params</code> is stored, as when a strategy's specs call <code>authorize_params</code> directly.</li>
<li>Session entries created in test mode now expire after the configured TTL instead of living forever. (<a href="https://example.invalid/acme/acme-auth/issues/195">#195</a>)</li>
<li>Let strategies keep the origin and params of several sign-ins in progress at once, for example in several tabs, so each callback gets its own instead of the latest one. A strategy opts in by calling <code>persist_login</code> in its request phase with a value the provider sends back, such as OAuth 2.0's state, and overriding <code>callback_session_key</code>. The latest 5 are stored in <code>session['acme_auth.logins']</code>. (<a href="https://example.invalid/acme/legacy-auth/issues/15">acme-legacy#15</a>)</li>
<li>Add a <code>full_host</code> option for each strategy, as a string or a proc that takes the request env. It takes precedence over <code>acme_auth.config.full_host</code>. (<a href="https://example.invalid/acme/acme-auth/issues/235">#235</a>)</li>
<li>Ship RBS type signatures for acme-auth in <code>sig/</code>.</li>
<li>Document the request env keys acme-auth sets, CSRF protection, sending <code>origin</code> as a form field, and how <code>SameSite</code> session cookies affect the callback in the README. (<a href="https://example.invalid/acme/acme-auth/issues/309">#309</a>, <a href="https://example.invalid/acme/acme-auth/pull/375">#375</a>)</li>
</ul>
<h2>3.1.4</h2>
<ul>
<li><code>acme_auth.params</code> now includes params posted to the request phase's form, not just its query string, so they survive the POST-only request phase introduced in 3.0. The <code>authenticity_token</code> param is never included. (<a href="https://example.invalid/acme/acme-auth/issues/275">#275</a>)</li>
<li><code>AcmeAuth::Builder</code> builds its middleware stack on the first request and reuses it, instead of rebuilding every strategy on every request. Strategies added after the first request are ignored. (<a href="https://example.invalid/acme/acme-auth/issues/283">#283</a>)</li>
<li>Require ruby 3.0 or later (was 2.7). (<a href="https://example.invalid/acme/acme-auth/pull/167">#167</a>)</li>
</ul>
<h2>3.1.3</h2>
<ul>
<li>Guard against forged hosts in callback URLs. Unless a <code>full_host</code> is set, acme-auth builds the callback URL it sends to the provider from the request's <code>Host</code> or <code>X-Forwarded-Host</code> header, which a client can forge. The new <code>acme_auth.config.allowed_hosts</code>, a list of strings and regexps, makes the request and callback phases fail with <code>invalid_host</code> unless the host matches one of them. (<a href="https://example.invalid/acme/acme-auth/security/advisories/GHSA-xxxx-xxxx-xxxx">GHSA-xxxx-xxxx-xxxx</a>, CVE-2000-0001)</li>
<li>Run <code>request_validation_phase</code> before writing <code>acme_auth.params</code> to the session, so a cross-site POST rejected by the authenticity check can no longer store attacker-controlled params. (<a href="https://example.invalid/acme/acme-auth/pull/162">#162</a>)</li>
</ul>
<h2>Commits</h2>
<ul>
<li><a href="https://example.invalid/acme/acme-auth/commit/d000000000000000000000000000000000000001"><code>d000001</code></a> Release 3.2.0</li>
<li><a href="https://example.invalid/acme/acme-auth/commit/d000000000000000000000000000000000000002"><code>d000002</code></a> Let persist_login run before acme_auth.params is stored</li>
<li><a href="https://example.invalid/acme/acme-auth/commit/d000000000000000000000000000000000000003"><code>d000003</code></a> Release 3.1.4</li>
<li><a href="https://example.invalid/acme/acme-auth/commit/d000000000000000000000000000000000000004"><code>d000004</code></a> Require json in the sign-ins spec</li>
<li><a href="https://example.invalid/acme/acme-auth/commit/d000000000000000000000000000000000000005"><code>d000005</code></a> Let strategies keep several sign-ins' origins apart</li>
<li><a href="https://example.invalid/acme/acme-auth/commit/d000000000000000000000000000000000000006"><code>d000006</code></a> Stop shipping CHANGELOG.md in the gem</li>
<li><a href="https://example.invalid/acme/acme-auth/commit/d000000000000000000000000000000000000007"><code>d000007</code></a> Release 3.1.3</li>
<li><a href="https://example.invalid/acme/acme-auth/commit/d000000000000000000000000000000000000008"><code>d000008</code></a> Add allowed_hosts to guard against forged hosts</li>
<li>Additional commits viewable in <a href="https://example.invalid/acme/acme-auth/compare/v3.1.0...v3.2.0">compare view</a></li>
</ul>
</blockquote>
</details>
