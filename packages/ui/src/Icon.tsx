const paths = {
  database:
    'M4 6C4 2 20 2 20 6C20 10 4 10 4 6ZM4 6V18C4 22 20 22 20 18V6M4 12C4 16 20 16 20 12',
  table: 'M3 4H21V20H3ZM3 9H21M9 9V20',
  folder: 'M3 7V4H10L13 7H21V20H3Z',
  plus: 'M12 4V20M4 12H20',
  chevron: 'M9 5L16 12L9 19',
  close: 'M6 6L18 18M18 6L6 18',
  play: 'M6 3L21 12L6 21Z',
  stop: 'M5 5H19V19H5Z',
  refresh: 'M20 7A9 9 0 1 0 21 15M20 3V9H14',
};
export function Icon({
  name,
  open = false,
}: {
  name: keyof typeof paths;
  open?: boolean;
}) {
  return (
    <svg
      className="ui-icon"
      viewBox="0 0 24 24"
      width="14"
      height="14"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      style={open ? { transform: 'rotate(90deg)' } : undefined}
    >
      <path d={paths[name]} />
    </svg>
  );
}
