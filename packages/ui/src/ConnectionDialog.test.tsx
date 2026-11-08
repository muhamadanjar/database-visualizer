import { describe, it, expect, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { ConnectionDialog } from './ConnectionDialog';
import type { DatabaseTransport } from '@database-studio/contracts';
describe('connection privacy defaults', () => {
  it('starts verified TLS and OS credential persistence enabled', () => {
    const transport = { request: vi.fn(), watch: vi.fn() } as DatabaseTransport;
    render(
      <ConnectionDialog
        transport={transport}
        onClose={() => {}}
        onSaved={() => {}}
      />,
    );
    expect(screen.getByLabelText('TLS')).toHaveValue('verify_full');
    const checkboxes = screen.getAllByRole('checkbox');
    expect(checkboxes[0]).toBeChecked();
    expect(checkboxes[1]).not.toBeChecked();
    expect(checkboxes[2]).not.toBeChecked();
  });
  it('allows an explicit memory-only save', async () => {
    const request = vi.fn().mockResolvedValue(null);
    const fillRequiredFields = () => {
      fireEvent.change(screen.getByLabelText('Nama'), {
        target: { value: 'Local analytics' },
      });
      fireEvent.change(screen.getByLabelText('Database'), {
        target: { value: 'analytics' },
      });
      fireEvent.change(screen.getByLabelText('Username'), {
        target: { value: 'studio' },
      });
      fireEvent.change(screen.getByLabelText('Password'), {
        target: { value: 'session-password' },
      });
    };
    render(
      <ConnectionDialog
        transport={{ request, watch: vi.fn() }}
        onClose={() => {}}
        onSaved={() => {}}
      />,
    );
    fireEvent.click(screen.getAllByRole('checkbox')[0]);
    fillRequiredFields();
    fireEvent.click(screen.getByText('Simpan koneksi'));
    await waitFor(() => expect(request).toHaveBeenCalled());
    expect(request).toHaveBeenCalledWith(
      'save_profile',
      expect.objectContaining({
        password: 'session-password',
        remember: false,
      }),
    );
    expect(request.mock.calls[0][1]).toEqual(
      expect.objectContaining({ remember: false }),
    );
  });
  it('reports save secret-store failures without closing form', async () => {
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
    fireEvent.change(screen.getByLabelText('Nama'), {
      target: { value: 'Local analytics' },
    });
    fireEvent.change(screen.getByLabelText('Database'), {
      target: { value: 'analytics' },
    });
    fireEvent.change(screen.getByLabelText('Username'), {
      target: { value: 'studio' },
    });
    fireEvent.change(screen.getByLabelText('Password'), {
      target: { value: 'stored-password' },
    });
    fireEvent.click(screen.getByText('Simpan koneksi'));
    await waitFor(() =>
      expect(screen.getByRole('status')).toHaveTextContent(
        'Unlock the OS credential store',
      ),
    );
    expect(request).toHaveBeenCalledWith(
      'save_profile',
      expect.objectContaining({ password: 'stored-password', remember: true }),
    );
    expect(onClose).not.toHaveBeenCalled();
  });
});
