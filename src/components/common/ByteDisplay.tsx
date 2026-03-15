import { formatBytes } from "../../lib/format";

interface ByteDisplayProps {
  bytes: number;
}

export function ByteDisplay({ bytes }: ByteDisplayProps) {
  return <span>{formatBytes(bytes)}</span>;
}
