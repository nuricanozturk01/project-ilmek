import { HttpClient, HttpErrorResponse, HttpParams } from '@angular/common/http';
import { Component, OnInit, inject, signal } from '@angular/core';
import { firstValueFrom } from 'rxjs';

interface Note {
  id: number;
  title: string;
  body: string;
}

interface ApiError {
  error?: string;
  fields?: Record<string, string>;
}

const NOTES = '/api/notes';

@Component({
  selector: 'app-root',
  templateUrl: './app.html',
})
export class App implements OnInit {
  private readonly http = inject(HttpClient);

  protected readonly notes = signal<Note[]>([]);
  protected readonly error = signal('');
  protected readonly editingId = signal<number | null>(null);
  protected readonly draftTitle = signal('');
  protected readonly draftBody = signal('');
  protected readonly query = signal('');

  async ngOnInit(): Promise<void> {
    await this.load();
  }

  protected async search(q: string): Promise<void> {
    this.query.set(q.trim());
    await this.load();
  }

  protected edit(note: Note): void {
    this.editingId.set(note.id);
    this.draftTitle.set(note.title);
    this.draftBody.set(note.body);
    this.error.set('');
  }

  protected cancel(): void {
    this.editingId.set(null);
    this.draftTitle.set('');
    this.draftBody.set('');
    this.error.set('');
  }

  protected async save(): Promise<void> {
    const payload = { title: this.draftTitle(), body: this.draftBody() };
    const id = this.editingId();
    const sent = await this.attempt(async () => {
      if (id === null) {
        await firstValueFrom(this.http.post<Note>(NOTES, payload));
      } else {
        await firstValueFrom(this.http.put<Note>(`${NOTES}/${id}`, payload));
      }
    });
    if (sent) {
      this.cancel();
      await this.load();
    }
  }

  protected async remove(note: Note): Promise<void> {
    const sent = await this.attempt(async () => {
      await firstValueFrom(this.http.delete<void>(`${NOTES}/${note.id}`));
    });
    if (sent) {
      if (this.editingId() === note.id) {
        this.cancel();
      }
      await this.load();
    }
  }

  private async load(): Promise<void> {
    const q = this.query();
    const params = q ? new HttpParams().set('q', q) : new HttpParams();
    await this.attempt(async () => {
      this.notes.set(await firstValueFrom(this.http.get<Note[]>(NOTES, { params })));
    });
  }

  private async attempt(action: () => Promise<void>): Promise<boolean> {
    try {
      await action();
      this.error.set('');
      return true;
    } catch (e) {
      this.error.set(describe(e));
      return false;
    }
  }
}

function describe(e: unknown): string {
  if (!(e instanceof HttpErrorResponse)) {
    return 'Something went wrong.';
  }
  const body = e.error as ApiError | null;
  const field = body?.fields ? Object.values(body.fields)[0] : undefined;
  if (field) {
    return `Title ${field}`;
  }
  return body?.error ?? `Request failed (${e.status}).`;
}
