#![allow(clippy::all, unused)]

use crate::tools::onpkg::stacks::{Stack, StackFile};

pub fn fastapi() -> Stack {
    Stack {
        hooks: vec![],
        name: "fastapi".into(),
        runtime: "uv".into(),
        description:
            "FastAPI + SQLAlchemy (Async) + Alembic + Pydantic v2 + structlog + Best Practices"
                .into(),
        packages: vec![
            "fastapi".into(),
            "uvicorn".into(),
            "sqlalchemy".into(),
            "asyncpg".into(),
            "alembic".into(),
            "pydantic-settings".into(),
            "python-dotenv".into(),
            "structlog".into(),
            "python-multipart".into(),
        ],
        dev_packages: vec![],
        transitive_packages: vec![],
        files: vec![
            // ── main.py ───────────────────────────────────────────────────────
            StackFile {
                path: "app/main.py".into(),
                content: r##"from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import PlainTextResponse
from fastapi.staticfiles import StaticFiles

from app.core.config import settings
from app.core.logger import setup_logging
from app.exceptions.handlers import add_exception_handlers
from app.middleware.request_logging import RequestLoggingMiddleware
from app.routes.router import api_router

setup_logging()

app = FastAPI(
    title=settings.app_name,
    version="1.0.0",
    description="FastAPI production-ready template",
)

app.add_middleware(RequestLoggingMiddleware)
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

add_exception_handlers(app)
app.include_router(api_router)

import os
os.makedirs("uploads", exist_ok=True)
app.mount("/uploads", StaticFiles(directory="uploads"), name="uploads")


@app.get("/", response_class=PlainTextResponse)
async def server_root():
    return "Server is running 🚀"


@app.get("/health")
async def health():
    return {
        "status": "ok",
        "message": f"Server is running in {settings.environment} mode",
        "app_name": settings.app_name,
    }


@app.get("/{full_path:path}")
async def not_found(full_path: str):
    return {"status": 404, "message": "Route not found", "path": f"/{full_path}"}
"##
                .into(),
                binary_content: None,
            },
            // ── core/config.py ────────────────────────────────────────────────
            StackFile {
                path: "app/core/config.py".into(),
                content: r##"from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    app_name: str = "FastAPI App"
    environment: str = "development"
    log_level: str = "INFO"
    database_url: str = "postgresql+asyncpg://postgres:postgres@localhost:5432/myapp"

    model_config = SettingsConfigDict(
        env_file=".env",
        env_file_encoding="utf-8",
        case_sensitive=False,
        extra="ignore",
    )


settings = Settings()
"##
                .into(),
                binary_content: None,
            },
            // ── core/logger.py ────────────────────────────────────────────────
            StackFile {
                path: "app/core/logger.py".into(),
                content: r##"import logging
import structlog


def setup_logging():
    logging.basicConfig(
        format="%(message)s",
        level=logging.INFO,
    )

    structlog.configure(
        processors=[
            structlog.processors.TimeStamper(fmt="iso"),
            structlog.processors.add_log_level,
            structlog.processors.format_exc_info,
            structlog.dev.ConsoleRenderer(),
        ],
        logger_factory=structlog.stdlib.LoggerFactory(),
        wrapper_class=structlog.stdlib.BoundLogger,
        cache_logger_on_first_use=True,
    )
"##
                .into(),
                binary_content: None,
            },
            // ── db/session.py ─────────────────────────────────────────────────
            StackFile {
                path: "app/db/session.py".into(),
                content:
                    r##"from sqlalchemy.ext.asyncio import create_async_engine, async_sessionmaker
from app.core.config import settings

engine = create_async_engine(
    settings.database_url,
    echo=False,
    pool_pre_ping=True,
)

SessionLocal = async_sessionmaker(
    bind=engine,
    autocommit=False,
    autoflush=False,
    expire_on_commit=False,
)
"##
                    .into(),
                binary_content: None,
            },
            // ── db/base.py ────────────────────────────────────────────────────
            StackFile {
                path: "app/db/base.py".into(),
                content: r##"from sqlalchemy.orm import DeclarativeBase


class Base(DeclarativeBase):
    """Base class for all SQLAlchemy models"""
    pass
"##
                .into(),
                binary_content: None,
            },
            // ── db/dependencies.py ────────────────────────────────────────────
            StackFile {
                path: "app/db/dependencies.py".into(),
                content: r##"from typing import AsyncGenerator
from sqlalchemy.ext.asyncio import AsyncSession
from app.db.session import SessionLocal


async def get_db() -> AsyncGenerator[AsyncSession, None]:
    async with SessionLocal() as db:
        yield db
"##
                .into(),
                binary_content: None,
            },
            // ── exceptions/handlers.py ────────────────────────────────────────
            StackFile {
                path: "app/exceptions/handlers.py".into(),
                content: r##"from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse


def add_exception_handlers(app: FastAPI):
    @app.exception_handler(Exception)
    async def global_exception_handler(request: Request, exc: Exception):
        return JSONResponse(
            status_code=500,
            content={"message": "Internal server error", "detail": str(exc)},
        )

    @app.exception_handler(404)
    async def not_found_handler(request: Request, exc: Exception):
        return JSONResponse(
            status_code=404,
            content={"message": "Resource not found"},
        )
"##
                .into(),
                binary_content: None,
            },
            // ── middleware/request_logging.py ─────────────────────────────────
            StackFile {
                path: "app/middleware/request_logging.py".into(),
                content: r##"import time
from datetime import datetime
from fastapi import Request
from starlette.middleware.base import BaseHTTPMiddleware
import logging
import sys

logging.basicConfig(level=logging.INFO, format="%(message)s", stream=sys.stdout)
logger = logging.getLogger(__name__)


class RequestLoggingMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request: Request, call_next):
        print(f"<-  {request.method} {request.url.path}")
        start = time.perf_counter()
        response = await call_next(request)
        duration = int((time.perf_counter() - start) * 1000)
        print(f"--> {request.method} {request.url.path} {response.status_code} {duration}ms")
        return response
"##
                .into(),
                binary_content: None,
            },
            // ── routes/router.py ──────────────────────────────────────────────
            StackFile {
                path: "app/routes/router.py".into(),
                content: r##"from fastapi import APIRouter

api_router = APIRouter(prefix="/api/v1")

# Import and include your feature routers here
# from app.routes import users
# api_router.include_router(users.router, prefix="/users", tags=["users"])
"##
                .into(),
                binary_content: None,
            },
            // ── models/__init__.py ────────────────────────────────────────────
            StackFile {
                path: "app/models/__init__.py".into(),
                content: "# Import all models here so Alembic can detect them\n".into(),
                binary_content: None,
            },
            // ── __init__ files ────────────────────────────────────────────────
            StackFile {
                path: "app/__init__.py".into(),
                content: "".into(),
                binary_content: None,
            },
            StackFile {
                path: "app/core/__init__.py".into(),
                content: "".into(),
                binary_content: None,
            },
            StackFile {
                path: "app/db/__init__.py".into(),
                content: "".into(),
                binary_content: None,
            },
            StackFile {
                path: "app/exceptions/__init__.py".into(),
                content: "".into(),
                binary_content: None,
            },
            StackFile {
                path: "app/middleware/__init__.py".into(),
                content: "".into(),
                binary_content: None,
            },
            StackFile {
                path: "app/routes/__init__.py".into(),
                content: "".into(),
                binary_content: None,
            },
            // ── alembic.ini ───────────────────────────────────────────────────
            StackFile {
                path: "alembic.ini".into(),
                content: r##"[alembic]
script_location = alembic
prepend_sys_path = .
sqlalchemy.url = postgresql+asyncpg://postgres:postgres@localhost:5432/myapp

[loggers]
keys = root,sqlalchemy,alembic

[handlers]
keys = console

[formatters]
keys = generic

[logger_root]
level = WARN
handlers = console
qualname =

[logger_sqlalchemy]
level = WARN
handlers =
qualname = sqlalchemy.engine

[logger_alembic]
level = INFO
handlers =
qualname = alembic

[handler_console]
class = StreamHandler
args = (sys.stderr,)
level = NOTSET
formatter = generic

[formatter_generic]
format = %(levelname)-5.5s [%(name)s] %(message)s
datefmt = %H:%M:%S
"##
                .into(),
                binary_content: None,
            },
            // ── alembic/env.py ────────────────────────────────────────────────
            StackFile {
                path: "alembic/env.py".into(),
                content: r##"import asyncio
from logging.config import fileConfig
from sqlalchemy import pool
from sqlalchemy.engine import Connection
from sqlalchemy.ext.asyncio import async_engine_from_config
from alembic import context
from app.core.config import settings
from app.db.base import Base
import app.models  # noqa: F401 - ensures all models are imported

config = context.config
config.set_main_option("sqlalchemy.url", settings.database_url)

if config.config_file_name is not None:
    fileConfig(config.config_file_name)

target_metadata = Base.metadata


def run_migrations_offline() -> None:
    url = config.get_main_option("sqlalchemy.url")
    context.configure(url=url, target_metadata=target_metadata, literal_binds=True)
    with context.begin_transaction():
        context.run_migrations()


def do_run_migrations(connection: Connection) -> None:
    context.configure(connection=connection, target_metadata=target_metadata)
    with context.begin_transaction():
        context.run_migrations()


async def run_async_migrations() -> None:
    connectable = async_engine_from_config(
        config.get_section(config.config_ini_section, {}),
        prefix="sqlalchemy.",
        poolclass=pool.NullPool,
    )
    async with connectable.connect() as connection:
        await connection.run_sync(do_run_migrations)
    await connectable.dispose()


def run_migrations_online() -> None:
    asyncio.run(run_async_migrations())


if context.is_offline_mode():
    run_migrations_offline()
else:
    run_migrations_online()
"##
                .into(),
                binary_content: None,
            },
            // ── alembic/script.py.mako ────────────────────────────────────────
            StackFile {
                path: "alembic/script.py.mako".into(),
                content: r##""""${message}

Revision ID: ${up_revision}
Revises: ${down_revision | comma,n}
Create Date: ${create_date}
"""
from typing import Sequence, Union
from alembic import op
import sqlalchemy as sa
${imports if imports else ""}

revision: str = ${repr(up_revision)}
down_revision: Union[str, None] = ${repr(down_revision)}
branch_labels: Union[str, Sequence[str], None] = ${repr(branch_labels)}
depends_on: Union[str, Sequence[str], None] = ${repr(depends_on)}


def upgrade() -> None:
    ${upgrades if upgrades else "pass"}


def downgrade() -> None:
    ${downgrades if downgrades else "pass"}
"##
                .into(),
                binary_content: None,
            },
            // ── alembic/versions/.gitkeep ─────────────────────────────────────
            StackFile {
                path: "alembic/versions/.gitkeep".into(),
                content: "".into(),
                binary_content: None,
            },
            // ── .env ──────────────────────────────────────────────────────────
            StackFile {
                path: ".env".into(),
                content:
                    r##"DATABASE_URL=postgresql+asyncpg://postgres:postgres@localhost:5432/myapp
ENVIRONMENT=development
LOG_LEVEL=INFO
"##
                    .into(),
                binary_content: None,
            },
            // ── justfile ──────────────────────────────────────────────────────
            StackFile {
                path: "justfile".into(),
                content: r##"default:
    just --list

dev:
    uv run uvicorn app.main:app --reload --port 8000

start:
    uv run uvicorn app.main:app --port 8000

migrate:
    uv run alembic upgrade head

makemigrations msg="New migration":
    uv run alembic revision --autogenerate -m "{{msg}}"

rollback:
    uv run alembic downgrade -1

reset-db:
    uv run alembic downgrade base && uv run alembic upgrade head
"##
                .into(),
                binary_content: None,
            },
            // ── uploads/.gitkeep ──────────────────────────────────────────────
            StackFile {
                path: "uploads/.gitkeep".into(),
                content: "".into(),
                binary_content: None,
            },
            // ── .gitignore ────────────────────────────────────────────────────
            StackFile {
                path: ".gitignore".into(),
                content: r##"__pycache__/
*.py[cod]
*.egg-info/
.venv/
.env
*.db
*.sqlite3
uploads/*
!uploads/.gitkeep
alembic/versions/*.py
!alembic/versions/.gitkeep
.DS_Store
"##
                .into(),
                binary_content: None,
            },
        ],
    }
}
