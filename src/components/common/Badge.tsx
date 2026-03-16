import { getStatusColor } from "../../lib/format";

interface BadgeProps {
  status: "OK" | "WARN" | "RETRY" | "FAIL";
}

export function Badge({ status }: BadgeProps) {
  return <span className={getStatusColor(status)}>{status}</span>;
}
