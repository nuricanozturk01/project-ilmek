import { ApplicationConfig, provideBrowserGlobalErrorListeners } from '@angular/core';
import { provideIlmekHttp } from '@ilmek/angular-http';

export const appConfig: ApplicationConfig = {
  providers: [provideBrowserGlobalErrorListeners(), provideIlmekHttp()],
};
