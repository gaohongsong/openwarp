ALTER TABLE ssh_servers ADD COLUMN proxy_jump TEXT;
ALTER TABLE ssh_servers ADD COLUMN connect_timeout_secs INTEGER;
ALTER TABLE ssh_servers ADD COLUMN keepalive_interval_secs INTEGER;
ALTER TABLE ssh_servers ADD COLUMN keepalive_count_max INTEGER;
ALTER TABLE ssh_servers ADD COLUMN source TEXT DEFAULT 'manual';
