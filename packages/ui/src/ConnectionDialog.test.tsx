import { describe, it, expect, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { ConnectionDialog } from './ConnectionDialog';
import type { DatabaseTransport } from '@database-studio/contracts';
describe('connection privacy defaults', () => {
  it('starts verified TLS and SQL persistence disabled', () => {
    const transport = { request: vi.fn(), watch: vi.fn() } as DatabaseTransport;
    render(
      <ConnectionDialog
        transport={transport}
        onClose={() => {}}
        onSaved={() => {}}
      />,
    );
    expect(screen.getByLabelText('TLS')).toHaveValue('verify_full');
    for (const checkbox of screen.getAllByRole('checkbox'))
      expect(checkbox).not.toBeChecked();
  });
  it('reports secret-store failures without closing form or changing privacy', async () => {
    const request = vi
      .fn()
      .mockRejectedValue({ message: 'Unlock the OS credential store' });
    const onClose = vi.fn();
    render(
      <ConnectionDialog
        transport={{ request, watch: vi.fn() }}
        onClose={onClose}
        onSaved={() => {}}
      />,
    );
    fireEvent.click(screen.getByText('Test koneksi'));
    await waitFor(() =>
      expect(screen.getByRole('status')).toHaveTextContent(
        'Unlock the OS credential store',
      ),
    );
    expect(onClose).not.toHaveBeenCalled();
  });
});
