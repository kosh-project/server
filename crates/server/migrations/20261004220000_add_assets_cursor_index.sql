CREATE INDEX IF NOT EXISTS idx_assets_user_tag_cursor
ON assets(user_id, tag, last_modified DESC, id DESC);
