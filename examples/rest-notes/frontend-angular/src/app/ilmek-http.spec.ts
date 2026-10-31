import { HttpClient } from '@angular/common/http';
import { TestBed } from '@angular/core/testing';
import { provideIlmekHttp } from '@ilmek/angular-http';
import { firstValueFrom } from 'rxjs';

describe('provideIlmekHttp with the real interceptor', () => {
  const call = vi.fn();

  beforeEach(() => {
    call.mockReset();
    Object.defineProperty(window, 'ilmek', { value: { call }, configurable: true, writable: true });
    TestBed.configureTestingModule({ providers: [provideIlmekHttp()] });
  });

  afterEach(() => {
    Reflect.deleteProperty(window, 'ilmek');
  });

  it('turns HttpClient GET /api/notes into ilmek.call http.read', async () => {
    call.mockResolvedValue({ status: 200, body: [{ id: 1, title: 'a', body: '' }] });
    const notes = await firstValueFrom(TestBed.inject(HttpClient).get<unknown[]>('/api/notes'));
    expect(notes).toEqual([{ id: 1, title: 'a', body: '' }]);
    expect(call).toHaveBeenCalledTimes(1);
    expect(call.mock.calls[0][0]).toBe('http.read');
    expect(call.mock.calls[0][1]).toEqual({ method: 'GET', path: '/api/notes' });
  });

  it('turns POST into http.write with the write option and a Location header', async () => {
    call.mockResolvedValue({
      status: 201,
      headers: { location: '/api/notes/5' },
      body: { id: 5, title: 't', body: '' },
    });
    const response = await firstValueFrom(
      TestBed.inject(HttpClient).post('/api/notes', { title: 't', body: '' }, { observe: 'response' }),
    );
    expect(response.status).toBe(201);
    expect(response.headers.get('location')).toBe('/api/notes/5');
    expect(call.mock.calls[0][0]).toBe('http.write');
    expect(call.mock.calls[0][1]).toMatchObject({
      method: 'POST',
      path: '/api/notes',
      body: { title: 't', body: '' },
    });
    expect(call.mock.calls[0][2]).toEqual({ write: true });
  });

  it('keeps the 400 body on the HttpErrorResponse', async () => {
    call.mockResolvedValue({
      status: 400,
      body: { error: 'validation failed', fields: { title: 'must not be blank' } },
    });
    await expect(firstValueFrom(TestBed.inject(HttpClient).post('/api/notes', {}))).rejects.toMatchObject({
      status: 400,
      error: { fields: { title: 'must not be blank' } },
    });
  });
});
