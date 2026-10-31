import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { App } from './app';

describe('App', () => {
  let fixture: ComponentFixture<App>;
  let http: HttpTestingController;

  const text = () => (fixture.nativeElement as HTMLElement).textContent ?? '';
  const el = (selector: string) =>
    (fixture.nativeElement as HTMLElement).querySelector(selector) as HTMLElement | null;

  async function settle(): Promise<void> {
    await fixture.whenStable();
    fixture.detectChanges();
  }

  async function start(initial: unknown[] = []): Promise<void> {
    fixture = TestBed.createComponent(App);
    fixture.detectChanges();
    http.expectOne('/api/notes').flush(initial);
    await settle();
  }

  function type(selector: string, value: string): void {
    const input = el(selector) as HTMLInputElement;
    input.value = value;
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
  }

  async function submit(): Promise<void> {
    el('form')?.dispatchEvent(new Event('submit'));
    await Promise.resolve();
  }

  beforeEach(() => {
    TestBed.configureTestingModule({
      imports: [App],
      providers: [provideHttpClient(), provideHttpClientTesting()],
    });
    http = TestBed.inject(HttpTestingController);
  });

  afterEach(() => http.verify());

  it('shows the empty state when there are no notes', async () => {
    await start([]);
    expect(text()).toContain('No notes yet.');
    expect(el('ul')).toBeNull();
  });

  it('lists the notes returned by GET /api/notes', async () => {
    await start([
      { id: 1, title: 'milk', body: '2 litres' },
      { id: 2, title: 'bread', body: '' },
    ]);
    expect(el('ul')?.querySelectorAll('li').length).toBe(2);
    expect(text()).toContain('milk');
    expect(text()).toContain('2 litres');
    expect(text()).not.toContain('No notes yet.');
  });

  it('creates a note with POST and reloads the list', async () => {
    await start([]);
    type('input[aria-label="Title"]', 'milk');
    type('textarea', '2 litres');
    await submit();
    const post = http.expectOne('/api/notes');
    expect(post.request.method).toBe('POST');
    expect(post.request.body).toEqual({ title: 'milk', body: '2 litres' });
    post.flush({ id: 1, title: 'milk', body: '2 litres' }, { status: 201, statusText: 'Created' });
    await Promise.resolve();
    await settle();
    http.expectOne('/api/notes').flush([{ id: 1, title: 'milk', body: '2 litres' }]);
    await settle();
    expect(text()).toContain('milk');
    expect((el('input[aria-label="Title"]') as HTMLInputElement).value).toBe('');
  });

  it('shows the validation message from a 400 body and keeps the draft', async () => {
    await start([]);
    type('input[aria-label="Title"]', '   ');
    await submit();
    http
      .expectOne('/api/notes')
      .flush(
        { error: 'validation failed', fields: { title: 'must not be blank' } },
        { status: 400, statusText: 'Bad Request' },
      );
    await settle();
    expect(el('.error')?.textContent).toContain('must not be blank');
    expect((el('input[aria-label="Title"]') as HTMLInputElement).value).toBe('   ');
  });

  it('edits a note with PUT /api/notes/{id}', async () => {
    await start([{ id: 7, title: 'old', body: 'text' }]);
    (fixture.nativeElement as HTMLElement).querySelector<HTMLButtonElement>('li .secondary')?.click();
    fixture.detectChanges();
    expect((el('input[aria-label="Title"]') as HTMLInputElement).value).toBe('old');
    type('input[aria-label="Title"]', 'new');
    await submit();
    const put = http.expectOne('/api/notes/7');
    expect(put.request.method).toBe('PUT');
    expect(put.request.body).toEqual({ title: 'new', body: 'text' });
    put.flush({ id: 7, title: 'new', body: 'text' });
    await Promise.resolve();
    await settle();
    http.expectOne('/api/notes').flush([{ id: 7, title: 'new', body: 'text' }]);
    await settle();
    expect(text()).toContain('new');
  });

  it('deletes a note with DELETE /api/notes/{id}', async () => {
    await start([{ id: 3, title: 'gone', body: '' }]);
    (fixture.nativeElement as HTMLElement).querySelector<HTMLButtonElement>('li .danger')?.click();
    const del = http.expectOne('/api/notes/3');
    expect(del.request.method).toBe('DELETE');
    del.flush(null, { status: 204, statusText: 'No Content' });
    await Promise.resolve();
    await settle();
    http.expectOne('/api/notes').flush([]);
    await settle();
    expect(text()).toContain('No notes yet.');
  });

  it('sends the search text as the q query parameter', async () => {
    await start([]);
    type('input[type="search"]', 'milk');
    const search = http.expectOne((r) => r.url === '/api/notes' && r.params.get('q') === 'milk');
    expect(search.request.method).toBe('GET');
    search.flush([]);
    await settle();
  });

  it('shows the error body message for a 404', async () => {
    await start([{ id: 9, title: 'x', body: '' }]);
    (fixture.nativeElement as HTMLElement).querySelector<HTMLButtonElement>('li .danger')?.click();
    http
      .expectOne('/api/notes/9')
      .flush({ error: 'note 9 not found' }, { status: 404, statusText: 'Not Found' });
    await settle();
    expect(el('.error')?.textContent).toContain('note 9 not found');
  });
});
