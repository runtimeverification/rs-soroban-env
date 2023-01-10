use crate::{
    convert::{EnvConvert, EnvConvertError},
    ConvertFrom, EnvConvertObject, Object, RawVal, Status,
};
use core::{borrow::Borrow};

impl<E, T, R> EnvConvertObject<Result<T, R>>
    for E
where
    E: EnvConvertObject<T>,
{
    fn to_object(&self, t: impl Borrow<Result<T, R>>) -> Result<crate::Object, Self::Error> {
        match t.borrow() {
            Ok(e) => <Self as EnvConvertObject<T>>::to_object(self, e),
            Err(_) => Err(self.ty_cvt_err::<Result<T, R>, Object>()),
        }
    }

    fn from_object(&self, obj: crate::Object) -> Result<Result<T, R>, Self::Error> {
        let t: T = <Self as EnvConvertObject<T>>::from_object(self, obj)?;
        Ok(Ok(t))
    }
}

impl<'a, C, T, R> ConvertFrom<Result<T, R>, C> for RawVal
where
    RawVal: ConvertFrom<T,C>,
    Status: From<&'a R>,
    R: 'a,
    C: EnvConvert<Result<T, R>,RawVal>,
    C: EnvConvert<T,RawVal>
{
    fn convert_from(
        t: impl Borrow<Result<T, R>>,
        c: &C,
    ) -> Result<Self, C::Error> {
        todo!()
/*         match t.borrow() {
            Ok(t) => RawVal::convert_from(t, c),
            Err(r) => {
                let status: Status = r.into();
                Err(status.into())
            }
        }
 */    }
}

impl<C, T, R> ConvertFrom<RawVal, C> for Result<T, R>
where
    T: ConvertFrom<RawVal, C>,
    R: From<Status>,
    C: EnvConvert<RawVal, Result<T, R>>,
    C: EnvConvert<RawVal, T>
{
    fn convert_from<>(
        t: impl Borrow<RawVal>,
        c: &C,
    ) -> Result<Self, C::Error> {
        if let Ok(status) = Status::try_from(*t.borrow()) {
            Ok(Err(status.into()))
        } else {
            // Do not collapse this Ok(Ok(x?)) into Ok(x); we want
            // a failure during the inner convert_from to turn into
            // an outermost Err, not an outermost Ok.
            let converted: T = T::convert_from(t, c)?;
            Ok(Ok(converted))
        }
    }
}
