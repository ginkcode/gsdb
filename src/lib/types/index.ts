export type DbDriver = "postgres" | "mysql" | "sqlite";

export interface SshTunnel {
  host: string;
  port: number;
  username: string;
  password?: string;
  privateKey?: string;
  privateKeyPassphrase?: string;
}

export interface Connection {
  id: string;
  name: string;
  driver: DbDriver;
  host?: string;
  port?: number;
  database: string;
  username?: string;
  password?: string;
  filePath?: string; // for SQLite
  ssh?: SshTunnel; // SSH tunnel configuration
}

export interface QueryResult {
  columns: string[];
  rows: Record<string, unknown>[];
  rowsAffected?: number;
  error?: string;
}

export interface QueryTab {
  id: string;
  connectionId: string;
  title: string;
  sql: string;
  result?: QueryResult;
  isLoading: boolean;
}
