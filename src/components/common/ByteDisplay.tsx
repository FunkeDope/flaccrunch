import { formatBytes } from "../../lib/format";

interface ByteDisplayProps {
  bytes: number;
  signed?: boolean;
}

export function ByteDisplay({ bytes, signed = false }: ByteDisplayProps) {
  return <span>{formatBytes(bytes, signed)}</span>;
}
