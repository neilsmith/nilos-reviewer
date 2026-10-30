[Task 4821](https://example.invalid/acme/app/tasks/4821)

## Team directory

Adds an optional "Team directory" module so workspaces can list members with
photos, roles and short bios on their public site.

### Behaviour

- **Model:** first name is required. Last name and image are optional. Each
  workspace's extra fields (role, bio, links…) come from its
  `directory.model` config through `SampleKit` `data_fields`, so adding a
  field needs no migration.
- **Config:** a `directory` block in `defaultv1`/`v2`/`v3` with `enabled`,
  `noun`, `sortable`, `hide_image`, `hide_hidden_field` and
  `after_commit_callbacks`.
- **Admin:** list, new, edit, delete and drag-to-reorder at
  `/admin/directory`, modelled on Documents and App Blocks. The image and
  hidden inputs follow `hide_image` and `hide_hidden_field`. The pages
  return a 404 when the module is off.
- **API v1:** `/api/v1/directory` (index and show). It needs both
  `api_v1.enabled` and `directory.enabled`. demo-site doesn't have API v1
  turned on, so it gets a 404 there.
- **App feed:** a `directory` key in `FeedJob`, only for workspaces with the
  module on. The image is left out when `hide_image` is set.
- **Fix:** `Web::V1::BaseController` raised `NoMethodError` for v1/v2
  workspaces, because their configs have no `api_v1` block. They now get a
  404.

### Test plan

1. Enable `directory.enabled` on demo-site, add three members and reorder
   them.
2. Confirm `/admin/directory` 404s with the module off.
3. Hit `/api/v1/directory` with and without `api_v1.enabled`.
4. Check the demo-site app feed: `directory` should list the visible members
   in order.
