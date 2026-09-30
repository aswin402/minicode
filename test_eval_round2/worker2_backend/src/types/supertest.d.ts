declare module 'supertest' {
  import { SuperTest, Test } from 'supertest';
  import { Application } from 'express';

  function supertest(app: Application): SuperTest<Test>;
  export = supertest;
}
