//! Integrate Diesel into Tokio cleanly and efficiently.
//!
//! ## Feature Flags
//!
//! - __tokio-rt-threaded__: Available when using the `rt-threaded` feature of tokio.
//! This feature will remove the `'static` lifetime restriction on the closures sent to the
//! `AsyncConnection` trait by using `tokio::task::block_in_place` instead of
//! `tokio::task::spawn_blocking`. It will also remove the `'static` restriction on the
//! `AsyncRunQueryDsl` and `AsyncSaveChangesDsl` implementations.

use async_trait::async_trait;
use diesel::{
    connection::SimpleConnection,
    dsl::Limit,
    query_dsl::{
        methods::{ExecuteDsl, LimitDsl, LoadQuery},
        RunQueryDsl, SaveChangesDsl, UpdateAndFetchResults,
    },
    r2d2::{ConnectionManager, Pool},
    result::QueryResult,
    Connection,
};
use std::{error::Error as StdError, fmt};
use tokio::task;

pub type AsyncResult<R> = Result<R, AsyncError>;

#[derive(Debug)]
pub enum AsyncError {
    // Failed to checkout a connection
    Checkout(r2d2::Error),

    // The query failed in some way
    Error(diesel::result::Error),
}

pub trait OptionalExtension<T> {
    fn optional(self) -> Result<Option<T>, AsyncError>;
}

impl<T> OptionalExtension<T> for AsyncResult<T> {
    fn optional(self) -> Result<Option<T>, AsyncError> {
        match self {
            Ok(value) => Ok(Some(value)),
            Err(AsyncError::Error(diesel::result::Error::NotFound)) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

impl fmt::Display for AsyncError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            AsyncError::Checkout(ref err) => err.fmt(f),
            AsyncError::Error(ref err) => err.fmt(f),
        }
    }
}

impl StdError for AsyncError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match *self {
            AsyncError::Checkout(ref err) => Some(err),
            AsyncError::Error(ref err) => Some(err),
        }
    }
}

#[async_trait]
pub trait AsyncSimpleConnection<Conn>
where
    Conn: 'static + SimpleConnection,
{
    async fn batch_execute_async(&self, query: &str) -> AsyncResult<()>;
}

#[async_trait]
impl<Conn> AsyncSimpleConnection<Conn> for Pool<ConnectionManager<Conn>>
where
    Conn: 'static + Connection,
{
    #[inline]
    async fn batch_execute_async(&self, query: &str) -> AsyncResult<()> {
        let self_ = self.clone();
        let query = query.to_string();
        task::spawn_blocking(move || {
            let conn = self_.get().map_err(AsyncError::Checkout)?;
            conn.batch_execute(&query).map_err(AsyncError::Error)
        })
        .await
        .expect("task has panicked")
    }
}

#[async_trait]
pub trait AsyncConnection<Conn>: AsyncSimpleConnection<Conn>
where
    Conn: 'static + Connection,
{
    #[cfg(not(feature = "tokio-rt-threaded"))]
    async fn run<R, Func>(&self, f: Func) -> AsyncResult<R>
    where
        R: 'static + Send,
        Func: 'static + FnOnce(&Conn) -> QueryResult<R> + Send;
    #[cfg(feature = "tokio-rt-threaded")]
    async fn run<R, Func>(&self, f: Func) -> AsyncResult<R>
    where
        R: 'static + Send,
        Func: FnOnce(&Conn) -> QueryResult<R> + Send;

    #[cfg(not(feature = "tokio-rt-threaded"))]
    async fn transaction<R, Func>(&self, f: Func) -> AsyncResult<R>
    where
        R: 'static + Send,
        Func: 'static + FnOnce(&Conn) -> QueryResult<R> + Send;
    #[cfg(feature = "tokio-rt-threaded")]
    async fn transaction<R, Func>(&self, f: Func) -> AsyncResult<R>
    where
        R: 'static + Send,
        Func: FnOnce(&Conn) -> QueryResult<R> + Send;
}

#[async_trait]
impl<Conn> AsyncConnection<Conn> for Pool<ConnectionManager<Conn>>
where
    Conn: 'static + Connection,
{
    #[cfg(not(feature = "tokio-rt-threaded"))]
    #[inline]
    async fn run<R, Func>(&self, f: Func) -> AsyncResult<R>
    where
        R: 'static + Send,
        Func: 'static + FnOnce(&Conn) -> QueryResult<R> + Send,
    {
        let self_ = self.clone();
        task::spawn_blocking(move || {
            let conn = self_.get().map_err(AsyncError::Checkout)?;
            f(&*conn).map_err(AsyncError::Error)
        })
        .await
        .expect("task has panicked")
    }
    #[cfg(feature = "tokio-rt-threaded")]
    #[inline]
    async fn run<R, Func>(&self, f: Func) -> AsyncResult<R>
    where
        R: 'static + Send,
        Func: FnOnce(&Conn) -> QueryResult<R> + Send,
    {
        let self_ = self.clone();
        task::block_in_place(move || {
            let conn = self_.get().map_err(AsyncError::Checkout)?;
            f(&*conn).map_err(AsyncError::Error)
        })
    }

    #[cfg(not(feature = "tokio-rt-threaded"))]
    #[inline]
    async fn transaction<R, Func>(&self, f: Func) -> AsyncResult<R>
    where
        R: 'static + Send,
        Func: 'static + FnOnce(&Conn) -> QueryResult<R> + Send,
    {
        let self_ = self.clone();
        task::spawn_blocking(move || {
            let conn = self_.get().map_err(AsyncError::Checkout)?;
            conn.transaction(|| f(&*conn)).map_err(AsyncError::Error)
        })
        .await
        .expect("task has panicked")
    }
    #[cfg(feature = "tokio-rt-threaded")]
    #[inline]
    async fn transaction<R, Func>(&self, f: Func) -> AsyncResult<R>
    where
        R: 'static + Send,
        Func: FnOnce(&Conn) -> QueryResult<R> + Send,
    {
        let self_ = self.clone();
        task::block_in_place(move || {
            let conn = self_.get().map_err(AsyncError::Checkout)?;
            conn.transaction(|| f(&*conn)).map_err(AsyncError::Error)
        })
    }
}

#[async_trait]
pub trait AsyncRunQueryDsl<Conn, AsyncConn>
where
    Conn: 'static + Connection,
{
    async fn execute_async(self, asc: &AsyncConn) -> AsyncResult<usize>
    where
        Self: ExecuteDsl<Conn>;

    async fn load_async<U>(self, asc: &AsyncConn) -> AsyncResult<Vec<U>>
    where
        U: 'static + Send,
        Self: LoadQuery<Conn, U>;

    async fn get_result_async<U>(self, asc: &AsyncConn) -> AsyncResult<U>
    where
        U: 'static + Send,
        Self: LoadQuery<Conn, U>;

    async fn get_results_async<U>(self, asc: &AsyncConn) -> AsyncResult<Vec<U>>
    where
        U: 'static + Send,
        Self: LoadQuery<Conn, U>;

    async fn first_async<U>(self, asc: &AsyncConn) -> AsyncResult<U>
    where
        U: 'static + Send,
        Self: LimitDsl,
        Limit<Self>: LoadQuery<Conn, U>;
}

#[cfg(not(feature = "tokio-rt-threaded"))]
#[async_trait]
impl<T, Conn> AsyncRunQueryDsl<Conn, Pool<ConnectionManager<Conn>>> for T
where
    T: 'static + Send + RunQueryDsl<Conn>,
    Conn: 'static + Connection,
{
    async fn execute_async(self, asc: &Pool<ConnectionManager<Conn>>) -> AsyncResult<usize>
    where
        Self: ExecuteDsl<Conn>,
    {
        asc.run(|conn| self.execute(&*conn)).await
    }

    async fn load_async<U>(self, asc: &Pool<ConnectionManager<Conn>>) -> AsyncResult<Vec<U>>
    where
        U: 'static + Send,
        Self: LoadQuery<Conn, U>,
    {
        asc.run(|conn| self.load(&*conn)).await
    }

    async fn get_result_async<U>(self, asc: &Pool<ConnectionManager<Conn>>) -> AsyncResult<U>
    where
        U: 'static + Send,
        Self: LoadQuery<Conn, U>,
    {
        asc.run(|conn| self.get_result(&*conn)).await
    }

    async fn get_results_async<U>(self, asc: &Pool<ConnectionManager<Conn>>) -> AsyncResult<Vec<U>>
    where
        U: 'static + Send,
        Self: LoadQuery<Conn, U>,
    {
        asc.run(|conn| self.get_results(&*conn)).await
    }

    async fn first_async<U>(self, asc: &Pool<ConnectionManager<Conn>>) -> AsyncResult<U>
    where
        U: 'static + Send,
        Self: LimitDsl,
        Limit<Self>: LoadQuery<Conn, U>,
    {
        asc.run(|conn| self.first(&*conn)).await
    }
}
#[cfg(feature = "tokio-rt-threaded")]
#[async_trait]
impl<T, Conn> AsyncRunQueryDsl<Conn, Pool<ConnectionManager<Conn>>> for T
where
    T: Send + RunQueryDsl<Conn>,
    Conn: 'static + Connection,
{
    async fn execute_async(self, asc: &Pool<ConnectionManager<Conn>>) -> AsyncResult<usize>
    where
        Self: ExecuteDsl<Conn>,
    {
        asc.run(|conn| self.execute(&*conn)).await
    }

    async fn load_async<U>(self, asc: &Pool<ConnectionManager<Conn>>) -> AsyncResult<Vec<U>>
    where
        U: 'static + Send,
        Self: LoadQuery<Conn, U>,
    {
        asc.run(|conn| self.load(&*conn)).await
    }

    async fn get_result_async<U>(self, asc: &Pool<ConnectionManager<Conn>>) -> AsyncResult<U>
    where
        U: 'static + Send,
        Self: LoadQuery<Conn, U>,
    {
        asc.run(|conn| self.get_result(&*conn)).await
    }

    async fn get_results_async<U>(self, asc: &Pool<ConnectionManager<Conn>>) -> AsyncResult<Vec<U>>
    where
        U: 'static + Send,
        Self: LoadQuery<Conn, U>,
    {
        asc.run(|conn| self.get_results(&*conn)).await
    }

    async fn first_async<U>(self, asc: &Pool<ConnectionManager<Conn>>) -> AsyncResult<U>
    where
        U: 'static + Send,
        Self: LimitDsl,
        Limit<Self>: LoadQuery<Conn, U>,
    {
        asc.run(|conn| self.first(&*conn)).await
    }
}

#[async_trait]
pub trait AsyncSaveChangesDsl<Conn, AsyncConn> {
    async fn save_changes_async<U>(self, asc: &AsyncConn) -> AsyncResult<U>
    where
        Self: Sized,
        U: 'static + Send,
        Conn: UpdateAndFetchResults<Self, U>;
}

#[cfg(not(feature = "tokio-rt-threaded"))]
#[async_trait]
impl<T, Conn> AsyncSaveChangesDsl<Conn, Pool<ConnectionManager<Conn>>> for T
where
    T: 'static + Send + SaveChangesDsl<Conn>,
    Conn: 'static + Connection,
{
    async fn save_changes_async<U>(self, asc: &Pool<ConnectionManager<Conn>>) -> AsyncResult<U>
    where
        U: 'static + Send,
        Conn: UpdateAndFetchResults<T, U>,
    {
        asc.run(|conn| self.save_changes(&*conn)).await
    }
}

#[cfg(feature = "tokio-rt-threaded")]
#[async_trait]
impl<T, Conn> AsyncSaveChangesDsl<Conn, Pool<ConnectionManager<Conn>>> for T
where
    T: Send + SaveChangesDsl<Conn>,
    Conn: 'static + Connection,
{
    async fn save_changes_async<U>(self, asc: &Pool<ConnectionManager<Conn>>) -> AsyncResult<U>
    where
        U: 'static + Send,
        Conn: UpdateAndFetchResults<T, U>,
    {
        asc.run(|conn| self.save_changes(&*conn)).await
    }
}
