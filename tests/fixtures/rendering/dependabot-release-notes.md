Bumps the acme-toolbelt group with 1 update in the / directory: [acme-toolbelt](https://example.invalid/acme/acme-toolbelt).

Updates `acme-toolbelt` from 4.2.0 to 4.3.1
<details>
<summary>Release notes</summary>
<p><em>Sourced from <a href="https://example.invalid/acme/acme-toolbelt/releases">acme-toolbelt's releases</a>.</em></p>
<blockquote>
<h2>4.3.1 Improvements</h2>
<ul>
<li>Revert "[cli] Support config profiles for init/enable/disable commands" (<a href="https://example.invalid/acme/acme-toolbelt/issues/149">#149</a>) via dev-mara (<a href="https://example.invalid/dev-mara"><code>@​dev-mara</code></a>)</li>
<li>[core] add tablet screenshot sizes for widget previews (<a href="https://example.invalid/acme/acme-toolbelt/issues/148">#148</a>) via dev-ivan (<a href="https://example.invalid/dev-ivan"><code>@​dev-ivan</code></a>)</li>
<li>[docs] Add device names for the new preview targets (<a href="https://example.invalid/acme/acme-toolbelt/issues/147">#147</a>) via dev-priya (<a href="https://example.invalid/dev-priya"><code>@​dev-priya</code></a>)</li>
<li>[ci] release: keep vendored caches out of the tracked config dir (<a href="https://example.invalid/acme/acme-toolbelt/issues/146">#146</a>) via dev-mara (<a href="https://example.invalid/dev-mara"><code>@​dev-mara</code></a>)</li>
<li>[ci] build: move openssl into a group the release job ignores (<a href="https://example.invalid/acme/acme-toolbelt/issues/145">#145</a>) via dev-ivan (<a href="https://example.invalid/dev-ivan"><code>@​dev-ivan</code></a>)</li>
</ul>
<h2>4.3.0 Improvements</h2>
<ul>
<li>[cli] ship: escape the tag message before invoking git (<a href="https://example.invalid/acme/acme-toolbelt/issues/142">#142</a>) via dev-mara (<a href="https://example.invalid/dev-mara"><code>@​dev-mara</code></a>)</li>
<li>[core] spec fixtures: build the verify apps at spec time instead of committing them (<a href="https://example.invalid/acme/acme-toolbelt/issues/140">#140</a>) via dev-priya (<a href="https://example.invalid/dev-priya"><code>@​dev-priya</code></a>)</li>
<li>[core] search plugins in transitive dependencies as well (<a href="https://example.invalid/acme/acme-toolbelt/issues/101">#101</a>) via dev-priya (<a href="https://example.invalid/dev-priya"><code>@​dev-priya</code></a>)</li>
<li>[cli] remove SnapshotRotate and SnapshotFlatten, unused since 2019 (<a href="https://example.invalid/acme/acme-toolbelt/issues/127">#127</a>) via dev-ivan (<a href="https://example.invalid/dev-ivan"><code>@​dev-ivan</code></a>)</li>
<li>[ci] release signed attested acme-toolbelt gem (<a href="https://example.invalid/acme/acme-toolbelt/issues/137">#137</a>) via dev-mara (<a href="https://example.invalid/dev-mara"><code>@​dev-mara</code></a>)</li>
<li>[cli] bump_version: escape the number given to the build tool (<a href="https://example.invalid/acme/acme-toolbelt/issues/128">#128</a>) via dev-ivan (<a href="https://example.invalid/dev-ivan"><code>@​dev-ivan</code></a>)</li>
<li>[cli] lint: escape the scanner's property values (<a href="https://example.invalid/acme/acme-toolbelt/issues/131">#131</a>) via dev-ivan (<a href="https://example.invalid/dev-ivan"><code>@​dev-ivan</code></a>)</li>
<li>[ci] remove creation of acme-toolbelt/* tag (<a href="https://example.invalid/acme/acme-toolbelt/issues/138">#138</a>) via dev-mara (<a href="https://example.invalid/dev-mara"><code>@​dev-mara</code></a>)</li>
<li>[ci] move to v0.8.0 of acme-toolbelt/github-actions (<a href="https://example.invalid/acme/acme-toolbelt/issues/139">#139</a>) via dev-mara (<a href="https://example.invalid/dev-mara"><code>@​dev-mara</code></a>)</li>
<li>[cli] escape the version options (<a href="https://example.invalid/acme/acme-toolbelt/issues/129">#129</a>) via dev-ivan (<a href="https://example.invalid/dev-ivan"><code>@​dev-ivan</code></a>)</li>
<li>[cli][core][docs] remove classes no code uses (<a href="https://example.invalid/acme/acme-toolbelt/issues/132">#132</a>) via dev-priya (<a href="https://example.invalid/dev-priya"><code>@​dev-priya</code></a>)</li>
<li>[core] remove Pilot::FlightInfo, its endpoint is gone (<a href="https://example.invalid/acme/acme-toolbelt/issues/133">#133</a>) via dev-priya (<a href="https://example.invalid/dev-priya"><code>@​dev-priya</code></a>)</li>
<li>[core] deprecate ResultParser, scan no longer uses it (<a href="https://example.invalid/acme/acme-toolbelt/issues/134">#134</a>) via dev-ivan (<a href="https://example.invalid/dev-ivan"><code>@​dev-ivan</code></a>)</li>
<li>[docs] add the OpenSSF Scorecard badge to the README (<a href="https://example.invalid/acme/acme-toolbelt/issues/135">#135</a>) via dev-priya (<a href="https://example.invalid/dev-priya"><code>@​dev-priya</code></a>)</li>
<li>[ci] release step 1: declare the write permissions on its job (<a href="https://example.invalid/acme/acme-toolbelt/issues/136">#136</a>) via dev-mara (<a href="https://example.invalid/dev-mara"><code>@​dev-mara</code></a>)</li>
</ul>
<h2>4.2.2 Fixes</h2>
<ul>
<li>[cli] pack: escape the paths it passes to tar and zip (<a href="https://example.invalid/acme/acme-toolbelt/issues/122">#122</a>) via dev-priya (<a href="https://example.invalid/dev-priya"><code>@​dev-priya</code></a>)</li>
<li>[cli] push_to_remote: escape push option values (<a href="https://example.invalid/acme/acme-toolbelt/issues/123">#123</a>) via dev-ivan (<a href="https://example.invalid/dev-ivan"><code>@​dev-ivan</code></a>)</li>
<li>[docs] correct the workspace flag examples (<a href="https://example.invalid/acme/acme-toolbelt/issues/124">#124</a>) via dev-mara (<a href="https://example.invalid/dev-mara"><code>@​dev-mara</code></a>)</li>
</ul>
</blockquote>
</details>
