interface HeaderProps {
  title: string;
}

export function Header({ title }: HeaderProps) {
  return (
    <div style={{ marginBottom: 20 }}>
      <h2 style={{ fontSize: 20, fontWeight: 700 }}>{title}</h2>
    </div>
  );
}
